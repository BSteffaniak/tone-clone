use switchy_database::{Database, DatabaseValue};

use crate::models::{Post, Stats};
use crate::Error;

/// Full-text search across posts.
pub async fn search(
    db: &dyn Database,
    terms: &str,
    exclude_ai: bool,
    post_types: Option<&[&str]>,
    limit: i64,
) -> Result<Vec<Post>, Error> {
    let mut conditions = vec!["posts_fts MATCH ?".to_string()];
    let mut params: Vec<DatabaseValue> = vec![DatabaseValue::String(terms.to_string())];

    if exclude_ai {
        conditions.push("p.likely_ai = 0".to_string());
    }

    if let Some(types) = post_types {
        if !types.is_empty() {
            let placeholders: Vec<&str> = types.iter().map(|_| "?").collect();
            conditions.push(format!("p.post_type IN ({})", placeholders.join(", ")));
            for t in types {
                params.push(DatabaseValue::String(t.to_string()));
            }
        }
    }

    params.push(DatabaseValue::Int64(limit));

    let where_clause = conditions.join(" AND ");
    let sql = format!(
        "SELECT p.* FROM posts p
         JOIN posts_fts ON posts_fts.rowid = p.id
         WHERE {where_clause}
         ORDER BY rank
         LIMIT ?"
    );

    let rows = db.query_raw_params(&sql, &params).await?;
    let mut posts = Vec::with_capacity(rows.len());
    for row in &rows {
        posts.push(Post::from_row(row)?);
    }
    Ok(posts)
}

/// Get aggregate stats about the database.
pub async fn stats(db: &dyn Database, source_id: Option<i64>) -> Result<Stats, Error> {
    let (where_clause, params) = if let Some(sid) = source_id {
        ("WHERE source_id = ?", vec![DatabaseValue::Int64(sid)])
    } else {
        ("", vec![])
    };

    // Total posts
    let total_rows = db
        .query_raw_params(
            &format!("SELECT COUNT(*) as cnt FROM posts {where_clause}"),
            &params,
        )
        .await?;
    let total_posts: i64 = total_rows
        .first()
        .and_then(|r| r.get("cnt"))
        .and_then(|v| match v {
            DatabaseValue::Int64(n) => Some(n),
            _ => None,
        })
        .unwrap_or(0);

    // Authentic posts
    let auth_params = if let Some(sid) = source_id {
        vec![DatabaseValue::Int64(sid)]
    } else {
        vec![]
    };
    let auth_where = if source_id.is_some() {
        "WHERE likely_ai = 0 AND source_id = ?"
    } else {
        "WHERE likely_ai = 0"
    };
    let auth_rows = db
        .query_raw_params(
            &format!("SELECT COUNT(*) as cnt FROM posts {auth_where}"),
            &auth_params,
        )
        .await?;
    let authentic_posts: i64 = auth_rows
        .first()
        .and_then(|r| r.get("cnt"))
        .and_then(|v| match v {
            DatabaseValue::Int64(n) => Some(n),
            _ => None,
        })
        .unwrap_or(0);

    let likely_ai_posts = total_posts - authentic_posts;

    // Posts by type
    let type_rows = db
        .query_raw_params(
            &format!(
                "SELECT post_type, COUNT(*) as cnt FROM posts {where_clause} GROUP BY post_type ORDER BY cnt DESC"
            ),
            &params,
        )
        .await?;
    let mut by_type = Vec::new();
    for row in &type_rows {
        if let (Some(DatabaseValue::String(pt)), Some(DatabaseValue::Int64(cnt))) =
            (row.get("post_type"), row.get("cnt"))
        {
            by_type.push((pt, cnt));
        }
    }

    // Posts by source
    let source_rows = db
        .query_raw_params(
            &format!(
                "SELECT s.platform, s.username, COUNT(*) as cnt
                 FROM posts p JOIN sources s ON p.source_id = s.id
                 {where_clause}
                 GROUP BY s.id ORDER BY cnt DESC"
            ),
            // Note: where_clause references source_id on posts, rewrite for join
            &params,
        )
        .await?;
    let mut by_source = Vec::new();
    for row in &source_rows {
        if let (
            Some(DatabaseValue::String(platform)),
            Some(DatabaseValue::String(username)),
            Some(DatabaseValue::Int64(cnt)),
        ) = (row.get("platform"), row.get("username"), row.get("cnt"))
        {
            by_source.push((platform, username, cnt));
        }
    }

    Ok(Stats {
        total_posts,
        authentic_posts,
        likely_ai_posts,
        by_type,
        by_source,
    })
}
