//! Package cache management

use crate::{NeuropmConfig, NeuropmResult};

pub struct PackageCache {
    #[allow(dead_code)]
    config: NeuropmConfig,
}

impl PackageCache {
    pub fn new(config: &NeuropmConfig) -> NeuropmResult<Self> {
        Ok(Self {
            config: config.clone(),
        })
    }
    
    pub async fn get(&self, _key: &str) -> Option<Vec<u8>> {
        None
    }
    
    pub async fn put(&self, _key: &str, _data: Vec<u8>) -> NeuropmResult<()> {
        Ok(())
    }
    
    pub async fn clear(&self) -> NeuropmResult<()> {
        Ok(())
    }
}