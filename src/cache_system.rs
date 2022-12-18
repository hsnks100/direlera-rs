use crate::room::*;
use log::{error, info, log_enabled, trace, warn, Level, LevelFilter};
use std::collections::HashMap;
#[derive(Debug)]
pub struct CacheSystem {
    pub incoming_data_vec: Vec<Vec<u8>>, // position, data
}

impl CacheSystem {
    pub fn new() -> CacheSystem {
        CacheSystem {
            incoming_data_vec: Vec::new(), // vec![Vec::new(); 256],
        }
    }
    pub fn reset(&mut self) {
        self.incoming_data_vec.clear(); // vec![Vec::new(); 256];
    }
    pub fn get_cache_position(&self, b: Vec<u8>) -> Result<u8, KailleraError> {
        let p = self.incoming_data_vec.iter().position(|x| x == &b);
        match p {
            Some(s) => Ok(s as u8),
            None => Err(KailleraError::NotFound),
        }
        // match self.incoming_hit_cache.get(&b) {
        //     Some(s) => Ok(*s),
        //     None => {
        //         return Err(KailleraError::NotFound);
        //     }
        // }
    }
    pub fn put_data(self: &mut Self, b: Vec<u8>) -> u8 {
        let p = self.get_cache_position(b.clone());
        match p {
            Ok(s) => return s,
            Err(e) => {
                if self.incoming_data_vec.len() < 256 {
                    self.incoming_data_vec.push(b.clone());
                    return (self.incoming_data_vec.len() - 1) as u8;
                } else {
                    self.incoming_data_vec.remove(0);
                    self.incoming_data_vec.push(b.clone());
                    return (self.incoming_data_vec.len() - 1) as u8;
                }
            }
        }
    }
    pub fn get_data(&self, pos: u8) -> Result<Vec<u8>, KailleraError> {
        match self.incoming_data_vec.get(pos as usize) {
            Some(s) => Ok(s.clone()),
            None => Err(KailleraError::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_test_will_pass() {
        let mut cs = CacheSystem::new();
        cs.put_data(vec![1, 2, 3, 4]);
        let cp = cs.get_cache_position(vec![1, 2, 3, 4]);
        match cp {
            Ok(s) => assert_eq!(s, 0),
            Err(e) => assert_eq!(1, 0),
        }
        cs.put_data(vec![1, 2, 3, 4]);
        let cp = cs.get_cache_position(vec![1, 2, 3, 4]);
        match cp {
            Ok(s) => assert_eq!(s, 0),
            Err(e) => assert_eq!(1, 0),
        }
    }
}
