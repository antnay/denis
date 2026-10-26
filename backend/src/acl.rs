use std::net::IpAddr;
use std::sync::Arc;

use arc_swap::ArcSwap;
use ipnet::IpNet;

use crate::db::{DbError, DbPool, ID_PK};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Allow,
    Deny,
    Refuse,
}

impl Action {
    fn parse(s: &str) -> Action {
        match s.trim().to_ascii_lowercase().as_str() {
            "deny" => Action::Deny,
            "refuse" => Action::Refuse,
            _ => Action::Allow,
        }
    }
}

struct AclEntry {
    action: Action,
    networks: Vec<IpNet>,
}

pub struct Acl {
    entries: ArcSwap<Vec<AclEntry>>,
}

impl Acl {
    pub async fn load(pool: &DbPool) -> Result<Arc<Self>, DbError> {
        ensure_table(pool).await?;
        Ok(Arc::new(Self {
            entries: ArcSwap::from_pointee(fetch(pool).await?),
        }))
    }

    #[allow(dead_code)]
    pub async fn reload(&self, pool: &DbPool) -> Result<(), DbError> {
        self.entries.store(Arc::new(fetch(pool).await?));
        Ok(())
    }

    #[inline]
    pub fn decide(&self, ip: IpAddr) -> Action {
        let entries = self.entries.load();
        if entries.is_empty() {
            return Action::Allow;
        }
        for e in entries.iter() {
            if e.networks.iter().any(|n| n.contains(&ip)) {
                return e.action;
            }
        }
        Action::Deny
    }
}

pub async fn ensure_table(pool: &DbPool) -> Result<(), DbError> {
    sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS access (
            id       {ID_PK},
            enabled  BOOLEAN NOT NULL DEFAULT TRUE,
            name     TEXT NOT NULL,
            action   TEXT NOT NULL,
            networks TEXT NOT NULL
        )"
    ))
    .execute(pool)
    .await?;
    Ok(())
}

async fn fetch(pool: &DbPool) -> Result<Vec<AclEntry>, DbError> {
    let rows: Vec<(bool, String, String)> =
        sqlx::query_as("SELECT enabled, action, networks FROM access ORDER BY id")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .filter(|(enabled, _, _)| *enabled)
        .filter_map(|(_, action, networks)| {
            let nets: Vec<IpNet> = networks
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            (!nets.is_empty()).then_some(AclEntry {
                action: Action::parse(&action),
                networks: nets,
            })
        })
        .collect())
}
