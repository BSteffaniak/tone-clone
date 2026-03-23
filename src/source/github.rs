use std::process::Command;

use async_trait::async_trait;
use serde_json::Value;
use switchy_database::{Database, DatabaseValue};

use super::{ScrapeResult, Scraper};
use crate::models::Source;
use crate::Error;

pub struct GitHubScraper;

impl GitHubScraper {
    pub fn new() -> Self {
        Self
    }

    /// Shell out to `gh api graphql` and return parsed JSON.
    fn graphql(&self, query: &str, variables: &Value) -> Result<Value, Error> {
        let mut cmd = Command::new("gh");
        cmd.args(["api", "graphql", "-f", &format!("query={query}")]);

        // Pass each variable as a separate -F flag
        if let Some(obj) = variables.as_object() {
            for (key, val) in obj {
                let val_str = match val {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                cmd.args(["-F", &format!("{key}={val_str}")]);
            }
        }

        let output = cmd
            .output()
            .map_err(|e| Error::Scrape(format!("failed to run gh: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Scrape(format!("gh api failed: {stderr}")));
        }

        let json: Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| Error::Scrape(format!("failed to parse gh output: {e}")))?;

        if let Some(errors) = json.get("errors") {
            return Err(Error::Scrape(format!("GraphQL errors: {errors}")));
        }

        Ok(json)
    }

    /// Insert a single post into the database, ignoring duplicates.
    /// Returns true if a new row was inserted.
    async fn insert_post(
        &self,
        db: &dyn Database,
        source: &Source,
        external_id: &str,
        post_type: &str,
        body: &str,
        url: Option<&str>,
        repo: Option<&str>,
        created_at: &str,
    ) -> Result<bool, Error> {
        // Skip empty bodies
        if body.trim().is_empty() {
            return Ok(false);
        }

        let likely_ai = if let Some(ref cutoff) = source.ai_cutoff_date {
            if created_at > cutoff.as_str() {
                1i64
            } else {
                0i64
            }
        } else {
            0i64
        };

        let result = db
            .exec_raw_params(
                "INSERT OR IGNORE INTO posts (source_id, external_id, post_type, body, url, repo, created_at, likely_ai)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                &[
                    DatabaseValue::Int64(source.id),
                    DatabaseValue::String(external_id.to_string()),
                    DatabaseValue::String(post_type.to_string()),
                    DatabaseValue::String(body.to_string()),
                    match url {
                        Some(u) => DatabaseValue::String(u.to_string()),
                        None => DatabaseValue::Null,
                    },
                    match repo {
                        Some(r) => DatabaseValue::String(r.to_string()),
                        None => DatabaseValue::Null,
                    },
                    DatabaseValue::String(created_at.to_string()),
                    DatabaseValue::Int64(likely_ai),
                ],
            )
            .await?;

        Ok(result > 0)
    }

    /// Scrape items where the user is the author.
    /// Fetches PR/issue bodies + the user's own comments on those items.
    async fn scrape_authored(
        &self,
        db: &dyn Database,
        source: &Source,
        resume_cursor: Option<&str>,
    ) -> Result<(i64, Option<String>), Error> {
        let mut count = 0i64;
        let mut cursor = resume_cursor.map(|s| s.to_string());

        loop {
            let after_clause = cursor
                .as_deref()
                .map(|c| format!(r#", after: "{c}""#))
                .unwrap_or_default();

            let query = format!(
                r#"query($searchQuery: String!) {{
                    search(query: $searchQuery, type: ISSUE, first: 50{after_clause}) {{
                        pageInfo {{
                            hasNextPage
                            endCursor
                        }}
                        nodes {{
                            ... on Issue {{
                                id
                                body
                                url
                                createdAt
                                repository {{ nameWithOwner }}
                                comments(first: 100) {{
                                    nodes {{
                                        id
                                        body
                                        url
                                        createdAt
                                        author {{ login }}
                                    }}
                                }}
                            }}
                            ... on PullRequest {{
                                id
                                body
                                url
                                createdAt
                                repository {{ nameWithOwner }}
                                comments(first: 100) {{
                                    nodes {{
                                        id
                                        body
                                        url
                                        createdAt
                                        author {{ login }}
                                    }}
                                }}
                                reviews(first: 50) {{
                                    nodes {{
                                        id
                                        body
                                        url
                                        createdAt
                                        author {{ login }}
                                        comments(first: 100) {{
                                            nodes {{
                                                id
                                                body
                                                url
                                                createdAt
                                                author {{ login }}
                                            }}
                                        }}
                                    }}
                                }}
                            }}
                        }}
                    }}
                }}"#
            );

            let variables = serde_json::json!({
                "searchQuery": format!("author:{}", source.username)
            });

            let json = self.graphql(&query, &variables)?;
            let search = &json["data"]["search"];
            let nodes = search["nodes"]
                .as_array()
                .ok_or_else(|| Error::Scrape("missing search nodes".into()))?;

            for node in nodes {
                let repo = node["repository"]["nameWithOwner"]
                    .as_str()
                    .unwrap_or_default();
                let is_pr = node.get("reviews").is_some();

                // Insert the authored body (PR or issue body)
                let post_type = if is_pr { "pr_body" } else { "issue_body" };
                if let (Some(id), Some(body), Some(url), Some(created)) = (
                    node["id"].as_str(),
                    node["body"].as_str(),
                    node["url"].as_str(),
                    node["createdAt"].as_str(),
                ) {
                    if self
                        .insert_post(
                            db,
                            source,
                            id,
                            post_type,
                            body,
                            Some(url),
                            Some(repo),
                            created,
                        )
                        .await?
                    {
                        count += 1;
                    }
                }

                // Insert user's comments on this item
                if let Some(comments) = node["comments"]["nodes"].as_array() {
                    for comment in comments {
                        if comment["author"]["login"].as_str() == Some(&source.username) {
                            let ct = if is_pr { "pr_comment" } else { "issue_comment" };
                            if let (Some(id), Some(body), Some(url), Some(created)) = (
                                comment["id"].as_str(),
                                comment["body"].as_str(),
                                comment["url"].as_str(),
                                comment["createdAt"].as_str(),
                            ) {
                                if self
                                    .insert_post(
                                        db,
                                        source,
                                        id,
                                        ct,
                                        body,
                                        Some(url),
                                        Some(repo),
                                        created,
                                    )
                                    .await?
                                {
                                    count += 1;
                                }
                            }
                        }
                    }
                }

                // For PRs: insert user's review bodies and review comments
                if let Some(reviews) = node.get("reviews").and_then(|r| r["nodes"].as_array()) {
                    for review in reviews {
                        if review["author"]["login"].as_str() == Some(&source.username) {
                            // Review body itself
                            if let (Some(id), Some(body), Some(created)) = (
                                review["id"].as_str(),
                                review["body"].as_str(),
                                review["createdAt"].as_str(),
                            ) {
                                if !body.is_empty() {
                                    let url = review["url"].as_str();
                                    if self
                                        .insert_post(
                                            db,
                                            source,
                                            id,
                                            "review_body",
                                            body,
                                            url,
                                            Some(repo),
                                            created,
                                        )
                                        .await?
                                    {
                                        count += 1;
                                    }
                                }
                            }

                            // Review inline comments
                            if let Some(rc) =
                                review.get("comments").and_then(|c| c["nodes"].as_array())
                            {
                                for comment in rc {
                                    if comment["author"]["login"].as_str() == Some(&source.username)
                                    {
                                        if let (Some(id), Some(body), Some(url), Some(created)) = (
                                            comment["id"].as_str(),
                                            comment["body"].as_str(),
                                            comment["url"].as_str(),
                                            comment["createdAt"].as_str(),
                                        ) {
                                            if self
                                                .insert_post(
                                                    db,
                                                    source,
                                                    id,
                                                    "review_comment",
                                                    body,
                                                    Some(url),
                                                    Some(repo),
                                                    created,
                                                )
                                                .await?
                                            {
                                                count += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let page_info = &search["pageInfo"];
            if page_info["hasNextPage"].as_bool() == Some(true) {
                cursor = page_info["endCursor"].as_str().map(|s| s.to_string());
                eprintln!("  authored: {count} posts so far, fetching next page...");
            } else {
                cursor = None;
                break;
            }
        }

        Ok((count, cursor))
    }

    /// Scrape items where the user commented but didn't author.
    async fn scrape_commented(&self, db: &dyn Database, source: &Source) -> Result<i64, Error> {
        let mut count = 0i64;
        let mut cursor: Option<String> = None;

        loop {
            let after_clause = cursor
                .as_deref()
                .map(|c| format!(r#", after: "{c}""#))
                .unwrap_or_default();

            let query = format!(
                r#"query($searchQuery: String!) {{
                    search(query: $searchQuery, type: ISSUE, first: 50{after_clause}) {{
                        pageInfo {{
                            hasNextPage
                            endCursor
                        }}
                        nodes {{
                            ... on Issue {{
                                repository {{ nameWithOwner }}
                                comments(first: 100) {{
                                    nodes {{
                                        id
                                        body
                                        url
                                        createdAt
                                        author {{ login }}
                                    }}
                                }}
                            }}
                            ... on PullRequest {{
                                repository {{ nameWithOwner }}
                                comments(first: 100) {{
                                    nodes {{
                                        id
                                        body
                                        url
                                        createdAt
                                        author {{ login }}
                                    }}
                                }}
                                reviews(first: 50) {{
                                    nodes {{
                                        id
                                        body
                                        url
                                        createdAt
                                        author {{ login }}
                                        comments(first: 100) {{
                                            nodes {{
                                                id
                                                body
                                                url
                                                createdAt
                                                author {{ login }}
                                            }}
                                        }}
                                    }}
                                }}
                            }}
                        }}
                    }}
                }}"#
            );

            let variables = serde_json::json!({
                "searchQuery": format!("commenter:{} -author:{}", source.username, source.username)
            });

            let json = self.graphql(&query, &variables)?;
            let search = &json["data"]["search"];
            let nodes = search["nodes"]
                .as_array()
                .ok_or_else(|| Error::Scrape("missing search nodes".into()))?;

            for node in nodes {
                let repo = node["repository"]["nameWithOwner"]
                    .as_str()
                    .unwrap_or_default();
                let is_pr = node.get("reviews").is_some();

                // Only grab the user's comments (not the body since they didn't author it)
                if let Some(comments) = node["comments"]["nodes"].as_array() {
                    for comment in comments {
                        if comment["author"]["login"].as_str() == Some(&source.username) {
                            let ct = if is_pr { "pr_comment" } else { "issue_comment" };
                            if let (Some(id), Some(body), Some(url), Some(created)) = (
                                comment["id"].as_str(),
                                comment["body"].as_str(),
                                comment["url"].as_str(),
                                comment["createdAt"].as_str(),
                            ) {
                                if self
                                    .insert_post(
                                        db,
                                        source,
                                        id,
                                        ct,
                                        body,
                                        Some(url),
                                        Some(repo),
                                        created,
                                    )
                                    .await?
                                {
                                    count += 1;
                                }
                            }
                        }
                    }
                }

                // PR review bodies and review comments
                if let Some(reviews) = node.get("reviews").and_then(|r| r["nodes"].as_array()) {
                    for review in reviews {
                        if review["author"]["login"].as_str() == Some(&source.username) {
                            if let (Some(id), Some(body), Some(created)) = (
                                review["id"].as_str(),
                                review["body"].as_str(),
                                review["createdAt"].as_str(),
                            ) {
                                if !body.is_empty() {
                                    let url = review["url"].as_str();
                                    if self
                                        .insert_post(
                                            db,
                                            source,
                                            id,
                                            "review_body",
                                            body,
                                            url,
                                            Some(repo),
                                            created,
                                        )
                                        .await?
                                    {
                                        count += 1;
                                    }
                                }
                            }

                            if let Some(rc) =
                                review.get("comments").and_then(|c| c["nodes"].as_array())
                            {
                                for comment in rc {
                                    if comment["author"]["login"].as_str() == Some(&source.username)
                                    {
                                        if let (Some(id), Some(body), Some(url), Some(created)) = (
                                            comment["id"].as_str(),
                                            comment["body"].as_str(),
                                            comment["url"].as_str(),
                                            comment["createdAt"].as_str(),
                                        ) {
                                            if self
                                                .insert_post(
                                                    db,
                                                    source,
                                                    id,
                                                    "review_comment",
                                                    body,
                                                    Some(url),
                                                    Some(repo),
                                                    created,
                                                )
                                                .await?
                                            {
                                                count += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let page_info = &search["pageInfo"];
            if page_info["hasNextPage"].as_bool() == Some(true) {
                cursor = page_info["endCursor"].as_str().map(|s| s.to_string());
                eprintln!("  commented: {count} posts so far, fetching next page...");
            } else {
                break;
            }
        }

        Ok(count)
    }
}

#[async_trait]
impl Scraper for GitHubScraper {
    fn platform(&self) -> &str {
        "github"
    }

    async fn scrape(
        &self,
        db: &dyn Database,
        source: &Source,
        resume_cursor: Option<&str>,
    ) -> Result<ScrapeResult, Error> {
        eprintln!("scraping github for @{} ...", source.username);

        // Pass 1: items authored by the user
        eprintln!("  pass 1: authored items");
        let (authored_count, _cursor) = self.scrape_authored(db, source, resume_cursor).await?;
        eprintln!("  pass 1 done: {authored_count} new posts");

        // Pass 2: items where user commented but didn't author
        eprintln!("  pass 2: commented items");
        let commented_count = self.scrape_commented(db, source).await?;
        eprintln!("  pass 2 done: {commented_count} new posts");

        let total = authored_count + commented_count;
        eprintln!("scrape complete: {total} new posts total");

        Ok(ScrapeResult {
            posts_fetched: total,
            cursor: None, // fully completed
        })
    }
}
