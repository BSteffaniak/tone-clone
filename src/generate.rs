use std::path::{Path, PathBuf};

use switchy_database::Database;

use crate::analyze::{self, TypeSummary, VoiceProfile};
use crate::models::Post;
use crate::query;
use crate::Error;

/// Options for the generate command.
pub struct GenerateOpts {
    pub output_dir: PathBuf,
    pub stdout: bool,
    pub post_type: Option<String>,
    pub topic: Option<String>,
    pub limit: usize,
    pub exclude_ai: bool,
    pub source_id: Option<i64>,
}

impl Default for GenerateOpts {
    fn default() -> Self {
        let data_dir = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            output_dir: data_dir.join("tone-clone").join("profiles"),
            stdout: false,
            post_type: None,
            topic: None,
            limit: 10,
            exclude_ai: true,
            source_id: None,
        }
    }
}

/// Run the generate command.
pub async fn run(db: &dyn Database, opts: &GenerateOpts) -> Result<(), Error> {
    let posts = query::fetch_posts(
        db,
        opts.exclude_ai,
        opts.post_type.as_deref(),
        opts.topic.as_deref(),
        opts.source_id,
    )
    .await?;

    if posts.is_empty() {
        eprintln!("no posts found matching filters.");
        return Ok(());
    }

    eprintln!("analyzing {} posts...", posts.len());

    let bodies: Vec<&str> = posts.iter().map(|p| p.body.as_str()).collect();
    let profile = analyze::build_profile(&bodies);
    let type_summaries = analyze::type_summaries(&posts);

    if opts.stdout {
        // Stdout mode: concatenate everything
        let profile_md = render_voice_profile(&profile, &type_summaries);
        print!("{profile_md}");

        // Examples section
        let example_types = match &opts.post_type {
            Some(pt) => vec![pt.as_str()],
            None => {
                let mut types: Vec<&str> = type_summaries
                    .iter()
                    .map(|s| s.post_type.as_str())
                    .collect();
                types.sort();
                types
            }
        };

        for pt in &example_types {
            let type_posts: Vec<&Post> = posts.iter().filter(|p| p.post_type == *pt).collect();
            if type_posts.is_empty() {
                continue;
            }
            // Re-collect as owned refs for select_diverse_examples
            let owned_type_posts: Vec<Post> = type_posts.into_iter().cloned().collect();
            let examples = analyze::select_diverse_examples(&owned_type_posts, opts.limit);
            let examples_md = render_examples(pt, &examples);
            print!("\n{examples_md}");
        }
    } else {
        // File mode: write to output_dir
        std::fs::create_dir_all(&opts.output_dir).map_err(|e| {
            Error::Config(format!(
                "failed to create output dir {}: {e}",
                opts.output_dir.display()
            ))
        })?;

        // Write voice profile
        let profile_md = render_voice_profile(&profile, &type_summaries);
        let profile_path = opts.output_dir.join("voice-profile.md");
        write_file(&profile_path, &profile_md)?;
        println!("wrote {}", profile_path.display());

        // Write per-type example files
        let example_types: Vec<String> =
            type_summaries.iter().map(|s| s.post_type.clone()).collect();

        for pt in &example_types {
            let type_posts: Vec<Post> = posts
                .iter()
                .filter(|p| p.post_type == *pt)
                .cloned()
                .collect();
            if type_posts.is_empty() {
                continue;
            }
            let examples = analyze::select_diverse_examples(&type_posts, opts.limit);
            let examples_md = render_examples(pt, &examples);
            let filename = format!("examples-{pt}.md");
            let path = opts.output_dir.join(&filename);
            write_file(&path, &examples_md)?;
            println!("wrote {}", path.display());
        }
    }

    Ok(())
}

fn write_file(path: &Path, content: &str) -> Result<(), Error> {
    std::fs::write(path, content)
        .map_err(|e| Error::Config(format!("failed to write {}: {e}", path.display())))
}

fn render_voice_profile(profile: &VoiceProfile, type_summaries: &[TypeSummary]) -> String {
    let mut out = String::new();

    out.push_str("# Voice Profile\n\n");
    out.push_str(&format!(
        "Generated from {} authentic posts.\n\n",
        profile.total_posts
    ));

    // Overall word count stats
    out.push_str("## Word Count\n\n");
    out.push_str(&format!(
        "| Metric | Value |\n|---|---|\n| Average | {:.1} |\n| Median | {:.1} |\n| Min | {} |\n| Max | {} |\n\n",
        profile.word_count.avg,
        profile.word_count.median,
        profile.word_count.min,
        profile.word_count.max,
    ));

    // Sentence stats
    out.push_str("## Sentence Structure\n\n");
    out.push_str(&format!(
        "| Metric | Value |\n|---|---|\n| Avg words per sentence | {:.1} |\n| Avg sentences per post | {:.1} |\n\n",
        profile.sentence.avg_sentence_word_count,
        profile.sentence.avg_sentences_per_post,
    ));

    // Style patterns
    out.push_str("## Style Patterns\n\n");
    out.push_str(&format!(
        "| Pattern | Rate |\n|---|---|\n| Starts with lowercase | {:.0}% |\n| Contains contractions | {:.0}% |\n| Contains questions | {:.0}% |\n\n",
        profile.lowercase_start_rate * 100.0,
        profile.contraction_rate * 100.0,
        profile.question_rate * 100.0,
    ));

    // Punctuation inventory
    out.push_str("## Punctuation Inventory\n\n");
    out.push_str("| Char | Total uses | Posts containing |\n|---|---|---|\n");
    for entry in &profile.punctuation {
        let display = match entry.char {
            '`' => "`` ` ``".to_string(),
            '|' => r"\|".to_string(),
            c => format!("`{c}`"),
        };
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            display, entry.count, entry.posts_with
        ));
    }

    // Flag notable absences
    let has_semicolon = profile.punctuation.iter().any(|e| e.char == ';');
    let has_em_dash = profile
        .punctuation
        .iter()
        .any(|e| e.char == '\u{2014}' || e.char == '\u{2013}');
    if !has_semicolon || !has_em_dash {
        out.push_str("\n**Notable absences:** ");
        let mut absences = Vec::new();
        if !has_semicolon {
            absences.push("semicolons (`;`)");
        }
        if !has_em_dash {
            absences.push("em-dashes (`\u{2014}`) and en-dashes (`\u{2013}`)");
        }
        out.push_str(&absences.join(", "));
        out.push_str(" are not used in authentic writing.\n");
    }
    out.push('\n');

    // Bigrams
    if !profile.bigrams.is_empty() {
        out.push_str("## Common Bigrams\n\n");
        out.push_str("| Phrase | Count |\n|---|---|\n");
        for ng in &profile.bigrams {
            out.push_str(&format!("| {} | {} |\n", ng.words, ng.count));
        }
        out.push('\n');
    }

    // Trigrams
    if !profile.trigrams.is_empty() {
        out.push_str("## Common Trigrams\n\n");
        out.push_str("| Phrase | Count |\n|---|---|\n");
        for ng in &profile.trigrams {
            out.push_str(&format!("| {} | {} |\n", ng.words, ng.count));
        }
        out.push('\n');
    }

    // Per-type summaries
    out.push_str("## By Post Type\n\n");
    out.push_str(
        "| Type | Count | Avg words | Median words | Lowercase start | Questions |\n|---|---|---|---|---|---|\n",
    );
    for ts in type_summaries {
        out.push_str(&format!(
            "| {} | {} | {:.1} | {:.1} | {:.0}% | {:.0}% |\n",
            ts.post_type,
            ts.count,
            ts.word_count.avg,
            ts.word_count.median,
            ts.lowercase_start_rate * 100.0,
            ts.question_rate * 100.0,
        ));
    }
    out.push('\n');

    out
}

fn render_examples(post_type: &str, examples: &[&Post]) -> String {
    let mut out = String::new();

    out.push_str(&format!("# Examples: {post_type}\n\n"));
    out.push_str(&format!("{} curated examples.\n\n", examples.len()));

    for (i, post) in examples.iter().enumerate() {
        let repo = post.repo.as_deref().unwrap_or("unknown");
        let url = post.url.as_deref().unwrap_or("");
        out.push_str(&format!("## {} ({}, {})\n\n", i + 1, repo, post.created_at));
        if !url.is_empty() {
            out.push_str(&format!("{url}\n\n"));
        }
        out.push_str(&post.body);
        out.push_str("\n\n---\n\n");
    }

    out
}
