pub mod github;

use async_trait::async_trait;
use switchy_database::Database;

use crate::models::Source;
use crate::Error;

/// Trait that each scraping source (GitHub, etc.) must implement.
#[async_trait]
pub trait Scraper: Send + Sync {
    /// Human-readable platform name (e.g., "github").
    fn platform(&self) -> &str;

    /// Run a full scrape for this source, inserting posts into the database.
    /// Returns the number of new posts inserted.
    async fn scrape(
        &self,
        db: &dyn Database,
        source: &Source,
        resume_cursor: Option<&str>,
    ) -> Result<ScrapeResult, Error>;
}

#[derive(Debug)]
pub struct ScrapeResult {
    pub posts_fetched: i64,
    pub cursor: Option<String>,
}
