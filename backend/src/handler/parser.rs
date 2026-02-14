use std::time::Instant;

use bytes::{BufMut, BytesMut};
use ftlog::{debug, info};

use crate::handler::Query;

const QUESTION: usize = 12;

// #[derive(thiserror::Error, Debug)]
// pub enum ParseError {
//     #[error("error: {0}")]
//     Error(String),
// }

enum ParseState {
    Length,
    Scan,
}

pub struct Parser {}

impl Parser {
    // Should probably check if valid
    pub async fn parse_udp(data: &BytesMut) -> Query {
        // let id = self.parse_header(data);
        let (qname, idx) = Parser::parse_question(data).await;
        let qtype = Parser::parse_qtype(data, idx);
        let qname_str = String::from_utf8_lossy(&qname);

        if cfg!(debug_assertions) {
            debug!("qname bytes: {:?}", qname);
            debug!("qname string: {}", qname_str.to_string());
            debug!("qype string: {:02x}", qtype);
        }

        Query {
            name: qname_str.to_string(),
            query_type: hickory_proto::rr::RecordType::from(qtype),
            raw: data.clone(),
            answer_offset: idx + 5,
        }
    }

    // #[inline]
    // fn parse_header(&self, data: &[u8]) -> u16 {
    //     u16::from_be_bytes([data[0], data[1]])
    // }

    // Returns a Vec<u8> containing the dns packet qname and pointer to the last index of qname
    // Should probably check if valid
    #[inline]
    async fn parse_question(data: &BytesMut) -> (BytesMut, usize) {
        let mut idx = QUESTION;
        let mut len = 0;
        let mut state = ParseState::Length;
        // let mut buf = Vec::with_capacity(64);
        let mut buf = BytesMut::with_capacity(64);

        while data[idx] != 0x00 {
            match state {
                ParseState::Length => {
                    len = data[idx];
                    idx += 1;
                    state = ParseState::Scan
                }
                ParseState::Scan => {
                    let stop = idx + len as usize;
                    for i in idx..stop {
                        buf.put_u8(data[i]);
                    }
                    idx += len as usize;

                    let time = Instant::now();
                    if data[idx] != 0x00 {
                        buf.put_u8(46);
                    }
                    let delta = time.elapsed();
                    if cfg!(debug_assertions) {
                        info!("push period time: {:?}", delta);
                    }

                    state = ParseState::Length;
                }
            }
        }
        (buf, idx)
    }

    #[inline]
    fn parse_qtype(data: &BytesMut, idx: usize) -> u16 {
        u16::from_be_bytes([data[idx + 1], data[idx + 2]])
    }

    #[inline]
    pub const fn parse_ttl(data: &[u8], mut idx: usize) -> u32 {
        idx += 6;
        u32::from_be_bytes([data[idx], data[idx + 1], data[idx + 2], data[idx + 3]])
    }
}
