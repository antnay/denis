use crate::handler::Query;

const HEADER: usize = 12;
const MAX_LABEL: usize = 63;
const MAX_NAME: usize = 255;

#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    #[error("packet too short: {0} bytes")]
    TooShort(usize),
    #[error("truncated question section")]
    Truncated,
    #[error("label length {0} exceeds 63")]
    LabelTooLong(u8),
    #[error("qname exceeds 255 octets")]
    NameTooLong,
    #[error("compression pointer in question is not allowed")]
    CompressionPointer,
}

enum ParseState {
    Length,
    Scan,
}

pub struct Parser {}

impl Parser {
    pub async fn parse_udp(data: Vec<u8>) -> Result<Query, ParseError> {
        if data.len() < HEADER {
            return Err(ParseError::TooShort(data.len()));
        }

        let (qname, idx) = Parser::parse_question(&data).await?;

        let qtype = Parser::parse_qtype(&data, idx)?;
        let answer_offset = idx + 5;

        let qname_str = String::from_utf8_lossy(&qname);

        Ok(Query {
            name: qname_str.to_string(),
            query_type: hickory_proto::rr::RecordType::from(qtype),
            raw: data,
            answer_offset,
        })
    }

    #[inline]
    async fn parse_question(data: &[u8]) -> Result<(Vec<u8>, usize), ParseError> {
        let mut idx = HEADER;
        let mut len = 0usize;
        let mut state = ParseState::Length;
        let mut buf = Vec::with_capacity(64);

        loop {
            let byte = *data.get(idx).ok_or(ParseError::Truncated)?;
            if byte == 0x00 {
                break;
            }

            match state {
                ParseState::Length => {
                    if byte >= 0xC0 {
                        return Err(ParseError::CompressionPointer);
                    }
                    if byte as usize > MAX_LABEL {
                        return Err(ParseError::LabelTooLong(byte));
                    }
                    len = byte as usize;
                    idx += 1;
                    state = ParseState::Scan;
                }
                ParseState::Scan => {
                    let stop = idx + len;
                    let label = data.get(idx..stop).ok_or(ParseError::Truncated)?;
                    if buf.len() + label.len() + 1 > MAX_NAME {
                        return Err(ParseError::NameTooLong);
                    }
                    for byte in label {
                        buf.push(byte.to_ascii_lowercase());
                    }
                    idx = stop;

                    if *data.get(idx).ok_or(ParseError::Truncated)? != 0x00 {
                        buf.push(b'.');
                    }
                    state = ParseState::Length;
                }
            }
        }

        Ok((buf, idx))
    }

    #[inline]
    fn parse_qtype(data: &[u8], idx: usize) -> Result<u16, ParseError> {
        let hi = *data.get(idx + 1).ok_or(ParseError::Truncated)?;
        let lo = *data.get(idx + 2).ok_or(ParseError::Truncated)?;
        Ok(u16::from_be_bytes([hi, lo]))
    }

    #[inline]
    pub fn parse_ttl(data: &[u8], idx: usize) -> u32 {
        let start = idx + 6;
        match data.get(start..start + 4) {
            Some(b) => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::rr::RecordType;

    fn query_packet(name: &str) -> Vec<u8> {
        let mut p = vec![0u8; HEADER];
        p[5] = 1; // QDCOUNT = 1
        for label in name.split('.') {
            p.push(label.len() as u8);
            p.extend_from_slice(label.as_bytes());
        }
        p.push(0x00); // root terminator
        p.extend_from_slice(&1u16.to_be_bytes()); // QTYPE = A
        p.extend_from_slice(&1u16.to_be_bytes()); // QCLASS = IN
        p
    }

    #[tokio::test]
    async fn parses_valid_query() {
        let q = Parser::parse_udp(query_packet("Ads.Example.COM"))
            .await
            .expect("valid packet parses");
        assert_eq!(q.name, "ads.example.com");
        assert_eq!(q.query_type, RecordType::A);
    }

    #[tokio::test]
    async fn rejects_malformed_without_panicking() {
        assert!(matches!(
            Parser::parse_udp(Vec::new()).await,
            Err(ParseError::TooShort(0))
        ));
        assert!(Parser::parse_udp(vec![0u8; 5]).await.is_err());
        // Header only, no question and no root terminator -> Truncated.
        assert!(matches!(
            Parser::parse_udp(vec![0u8; HEADER]).await,
            Err(ParseError::Truncated)
        ));

        // Label claims 10 bytes but the packet ends after 3.
        let mut p = vec![0u8; HEADER];
        p.extend_from_slice(&[10, b'a', b'b', b'c']);
        assert!(matches!(
            Parser::parse_udp(p).await,
            Err(ParseError::Truncated)
        ));

        // No 0x00 terminator: labels run to end of buffer.
        let mut p = vec![0u8; HEADER];
        p.extend_from_slice(&[3, b'a', b'b', b'c']);
        assert!(matches!(
            Parser::parse_udp(p).await,
            Err(ParseError::Truncated)
        ));

        // Compression pointer (0xC0) where a label length is expected.
        let mut p = vec![0u8; HEADER];
        p.extend_from_slice(&[0xC0, 0x0C]);
        assert!(matches!(
            Parser::parse_udp(p).await,
            Err(ParseError::CompressionPointer)
        ));

        // Reserved length byte (0b01xxxxxx) -> LabelTooLong.
        let mut p = vec![0u8; HEADER];
        p.extend_from_slice(&[0x41, b'a']);
        assert!(matches!(
            Parser::parse_udp(p).await,
            Err(ParseError::LabelTooLong(0x41))
        ));

        // Valid labels but the packet ends before QTYPE.
        let mut p = vec![0u8; HEADER];
        p.extend_from_slice(&[3, b'a', b'b', b'c', 0x00]); // name, no qtype
        assert!(matches!(
            Parser::parse_udp(p).await,
            Err(ParseError::Truncated)
        ));
    }

    #[test]
    fn parse_ttl_never_panics_on_short_answer() {
        assert_eq!(Parser::parse_ttl(&[0u8; 4], 0), 0);
    }
}
