use moosicbox_json_utils::database::ToValue as _;
use switchy_database::Row;

#[derive(Debug, Clone)]
pub struct Source {
    pub id: i64,
    pub platform: String,
    pub username: String,
    pub ai_cutoff_date: Option<String>,
    pub created_at: String,
}

impl Source {
    pub fn from_row(row: &Row) -> Result<Self, moosicbox_json_utils::ParseError> {
        Ok(Self {
            id: row.to_value("id")?,
            platform: row.to_value("platform")?,
            username: row.to_value("username")?,
            ai_cutoff_date: row.to_value("ai_cutoff_date")?,
            created_at: row.to_value("created_at")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Post {
    pub id: i64,
    pub source_id: i64,
    pub external_id: String,
    pub post_type: String,
    pub body: String,
    pub url: Option<String>,
    pub repo: Option<String>,
    pub created_at: String,
    pub likely_ai: bool,
    pub scraped_at: String,
}

impl Post {
    pub fn from_row(row: &Row) -> Result<Self, moosicbox_json_utils::ParseError> {
        Ok(Self {
            id: row.to_value("id")?,
            source_id: row.to_value("source_id")?,
            external_id: row.to_value("external_id")?,
            post_type: row.to_value("post_type")?,
            body: row.to_value("body")?,
            url: row.to_value("url")?,
            repo: row.to_value("repo")?,
            created_at: row.to_value("created_at")?,
            likely_ai: {
                let val: i64 = row.to_value("likely_ai")?;
                val != 0
            },
            scraped_at: row.to_value("scraped_at")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ScrapeLogEntry {
    pub id: i64,
    pub source_id: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub posts_fetched: i64,
    pub cursor: Option<String>,
}

impl ScrapeLogEntry {
    pub fn from_row(row: &Row) -> Result<Self, moosicbox_json_utils::ParseError> {
        Ok(Self {
            id: row.to_value("id")?,
            source_id: row.to_value("source_id")?,
            started_at: row.to_value("started_at")?,
            finished_at: row.to_value("finished_at")?,
            posts_fetched: row.to_value("posts_fetched")?,
            cursor: row.to_value("cursor")?,
        })
    }
}

/// Stats summary for display
#[derive(Debug)]
pub struct Stats {
    pub total_posts: i64,
    pub authentic_posts: i64,
    pub likely_ai_posts: i64,
    pub by_type: Vec<(String, i64)>,
    pub by_source: Vec<(String, String, i64)>,
}
