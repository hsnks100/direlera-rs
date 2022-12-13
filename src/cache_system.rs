use crate::room::*;
use log::{error, info, log_enabled, trace, warn, Level, LevelFilter};
use std::collections::HashMap;
#[derive(Debug)]
pub struct CacheSystem {
    pub position: u8,
    pub incoming_data: HashMap<u8, Vec<u8>>,
    pub incoming_hit_cache: HashMap<Vec<u8>, u8>,
}

impl CacheSystem {
    pub fn new() -> CacheSystem {
        CacheSystem {
            position: 0,
            incoming_data: HashMap::new(),
            incoming_hit_cache: HashMap::new(),
        }
    }
    pub fn reset(&mut self) {
        self.position = 0;
        self.incoming_data.clear();
        self.incoming_hit_cache.clear();
    }
    pub fn get_cache_position(self, b: Vec<u8>) -> Result<u8, KailleraError> {
        match self.incoming_hit_cache.get(&b) {
            Some(s) => Ok(*s),
            None => {
                return Err(KailleraError::NotFound);
            }
        }
    }
    pub fn put_data(self: &mut Self, b: Vec<u8>) -> u8 {
        match self.incoming_hit_cache.get(&b) {
            Some(s) => *s,
            None => {
                self.incoming_data.insert(self.position, b.clone());
                self.incoming_hit_cache.insert(b, self.position);
                self.position += 1;
                if self.position >= 250 {
                    info!("warning cache");
                }
                self.position - 1
            }
        }
    }
    pub fn get_data(&self, pos: u8) -> Result<Vec<u8>, KailleraError> {
        match self.incoming_data.get(&pos) {
            Some(s) => Ok(s.clone()),
            None => Err(KailleraError::NotFound),
        }
    }
}
