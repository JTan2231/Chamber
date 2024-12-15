use chamber_common::Logger;
use chamber_common::{error, get_data_dir, info};

use crate::cache::EmbeddingCache;
use crate::dbio::BLOCK_SIZE;
use crate::hnsw::{Filter, Query, HNSW};
use crate::openai::{embed, EmbeddingSource};

mod cache;
pub mod config;
pub mod dbio;
pub mod hnsw;
pub mod ledger;
mod openai;
mod parsing;
pub mod serialization;
pub mod test_common;

pub struct Dewey {
    index: hnsw::HNSW,
    cache: EmbeddingCache,
}

impl Dewey {
    pub fn new() -> Result<Self, std::io::Error> {
        crate::config::setup();

        // basic index setup/initialization

        // TODO: error handling
        ledger::sync_ledger_config().unwrap();
        dbio::sync_index(false)?;

        {
            let index = hnsw::HNSW::new(true)?;
            index.serialize(&get_data_dir().join("index").to_str().unwrap().to_string())?;
        }

        dbio::reblock()?;

        Ok(Self {
            index: HNSW::new(true)?,
            cache: EmbeddingCache::new((20 * BLOCK_SIZE) as u32)?,
        })
    }

    // TODO: better define how filters should be passed
    pub fn query(
        &mut self,
        query: String,
        filters: Vec<String>,
        k: usize,
    ) -> Result<Vec<EmbeddingSource>, std::io::Error> {
        let timestamp = chrono::Utc::now().timestamp_micros();

        // TODO: better file handling + all that
        let path = std::path::PathBuf::from("/tmp")
            .join("dewey_queries")
            .join(timestamp.to_string());
        match std::fs::write(path.clone(), query) {
            Ok(_) => {
                info!("Wrote query to {}", path.to_string_lossy());
            }
            Err(e) => {
                error!(
                    "error writing query to file {}: {}",
                    path.to_string_lossy(),
                    e
                );
                return Err(e);
            }
        };

        let embedding = match embed(&EmbeddingSource {
            filepath: path.to_string_lossy().to_string(),
            meta: std::collections::HashSet::new(),
            subset: None,
        }) {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to create embedding: {}", e);
                return Err(std::io::Error::new(std::io::ErrorKind::Other, e));
            }
        };

        info!("embedding created");

        let filters = filters
            .iter()
            .map(|f| Filter::from_string(&f.to_string()).unwrap())
            .collect::<Vec<Filter>>();

        let query = Query { embedding, filters };

        Ok(self
            .index
            .query(&mut self.cache, &query, k, 200)
            .iter()
            .map(|p| p.0.source_file.clone())
            .collect())
    }

    // this returns an empty json object {} on success
    // or an object with just an `error` key on error
    pub fn reindex(&mut self, filepath: String) -> Result<(), std::io::Error> {
        crate::dbio::update_file_embeddings(&filepath, &mut self.index)
    }

    pub fn add_embedding(&mut self, filepath: String) -> Result<(), std::io::Error> {
        let mut embedding = embed(&EmbeddingSource {
            filepath,
            subset: None,
            meta: std::collections::HashSet::new(),
        })?;

        dbio::add_new_embedding(&mut embedding);

        self.index.insert(&mut self.cache, &embedding)?;

        Ok(())
    }
}
