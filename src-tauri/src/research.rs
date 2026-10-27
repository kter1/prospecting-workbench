use std::collections::HashSet;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use url::Url;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::matcher::{self, Text};
use crate::models::{Account, QualificationRule};
use crate::qualification::matches_keyword;
use crate::safety::{fetch_public, robots_allows, FetchResult};

#[derive(Debug, Clone)]
pub struct ResearchFact {
    pub source_url: String,
    pub source_kind: String,
    pub title: String,
    pub excerpt: String,
    pub confidence: String,
    pub etag: Option<String>,
    pub modified_at: Option<String>,
    pub contacts: Vec<(String, String, String)>,
    pub signals: Vec<(String, String, String)>,
}

/// How the crawler reaches the network. Production uses `PublicFetcher`, which
/// applies the full safety policy; tests substitute a fixture fetcher.
#[async_trait]
pub trait Fetcher: Send + Sync {
    async fn fetch(&self, url: Url) -> AppResult<FetchResult>;
}

pub struct PublicFetcher {
    pub client: Client,
}

#[async_trait]
impl Fetcher for PublicFetcher {
    async fn fetch(&self, url: Url) -> AppResult<FetchResult> {
        fetch_public(&self.client, url).await
    }
}

/// Recently fetched pages and robots files, so a repeat search or a re-score
/// with edited rules does not re-crawl sites it read moments ago.
pub trait PageCache: Send + Sync {
    fn robots(&self, host: &str) -> Option<String>;
    fn store_robots(&self, host: &str, text: &str);
    fn page(&self, account_id: &str, url: &Url) -> Option<FetchResult>;
    fn store_page(&self, account_id: &str, url: &Url, page: &FetchResult);
}

/// A cache that remembers nothing.
#[cfg(test)]
pub struct NoCache;

#[cfg(test)]
impl PageCache for NoCache {
    fn robots(&self, _: &str) -> Option<String> {
        None
    }
    fn store_robots(&self, _: &str, _: &str) {}
    fn page(&self, _: &str, _: &Url) -> Option<FetchResult> {
        None
    }
    fn store_page(&self, _: &str, _: &Url, _: &FetchResult) {}
}

/// Path fragments of pages that usually carry evidence about a business.
const PAGE_SLUGS: &[&str] = &[
    "about",
    "contact",
    "team",
    "leader",
    "career",
    "jobs",
    "news",
    "press",
    "blog",
    "services",
    "locations",
    "pricing",
    "menu",
    "solutions",
    "patients",
    "order",
];

/// A failure that will not change on retry within the cache window: the page
/// does not exist, is forbidden, or is not a readable document.
fn is_definitive_miss(error: &AppError) -> bool {
    let message = error.to_string();
    message.starts_with("Source returned HTTP 4")
        || message.starts_with("Unsafe or unsupported content type")
}

/// The cached stand-in for a definitive miss. Its empty text yields no facts.
fn empty_page(url: &Url) -> FetchResult {
    FetchResult {
        final_url: url.clone(),
        title: String::new(),
        text: String::new(),
        links: Vec::new(),
        etag: None,
        modified_at: None,
        technologies: Vec::new(),
    }
}

/// How long a robots file may take before it is treated as absent.
const ROBOTS_TIMEOUT: Duration = Duration::from_secs(2);

pub struct Crawl<'a> {
    pub fetcher: &'a dyn Fetcher,
    pub cache: &'a dyn PageCache,
    /// Rules whose supporting lines become evidence.
    pub focus: &'a [QualificationRule],
    /// Roles whose mention on a people page becomes evidence.
    pub personas: &'a [String],
    pub page_budget: usize,
    /// No new fetch starts after this. Facts gathered before it are kept.
    pub deadline: Instant,
}

impl Crawl<'_> {
    fn remaining(&self) -> Option<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
    }

    async fn robots_for(&self, url: &Url) -> String {
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        if let Some(text) = self.cache.robots(&host) {
            return text;
        }
        let Ok(robots_url) = url.join("/robots.txt") else {
            return String::new();
        };
        let limit = self.remaining().unwrap_or_default().min(ROBOTS_TIMEOUT);
        // A missing, failing or slow robots file allows crawling, as before;
        // it is never skipped, and it is always read before the pages it
        // governs.
        let text = match tokio::time::timeout(limit, self.fetcher.fetch(robots_url)).await {
            Ok(Ok(result)) => result.text,
            _ => String::new(),
        };
        self.cache.store_robots(&host, &text);
        text
    }

    /// A page from the cache, or fetched within the remaining time.
    async fn page(&self, account_id: &str, url: &Url) -> AppResult<(FetchResult, bool)> {
        if let Some(page) = self.cache.page(account_id, url) {
            return Ok((page, false));
        }
        let left = self
            .remaining()
            .ok_or_else(|| AppError::Message("Public research reached its time budget.".into()))?;
        let fetched = tokio::time::timeout(left, self.fetcher.fetch(url.clone()))
            .await
            .map_err(|_| AppError::Message("Public research reached its time budget.".into()))?;
        let page = match fetched {
            Ok(page) => page,
            Err(error) => {
                // A missing page is as cacheable as a present one: without
                // this, every re-score re-requested each guessed path that
                // 404'd the first time. Transient failures are not cached.
                if is_definitive_miss(&error) {
                    self.cache.store_page(account_id, url, &empty_page(url));
                }
                return Err(error);
            }
        };
        self.cache.store_page(account_id, url, &page);
        Ok((page, true))
    }

    pub async fn run(&self, account: &Account) -> AppResult<Vec<ResearchFact>> {
        let root = account
            .website_url
            .as_deref()
            .or(account.domain.as_deref())
            .ok_or_else(|| {
                AppError::Message("This account has no public website to research.".into())
            })?;
        let root = if root.contains("://") {
            root.to_string()
        } else {
            format!("https://{root}")
        };
        let root = Url::parse(&root)?;
        let robots = self.robots_for(&root).await;
        if !robots.is_empty() && !robots_allows(&robots, root.path()) {
            return Err(AppError::Message(
                "The company's robots.txt does not permit reading its homepage.".into(),
            ));
        }
        let (homepage, _) = self.page(&account.id, &root).await?;
        let mut urls = vec![root.clone()];
        let root_host = root
            .host_str()
            .unwrap_or_default()
            .trim_start_matches("www.");
        if account.domain.is_some() {
            // Prefer links the company actually publishes, ranked by how likely
            // they are to hold evidence for the approved rules. Guessed paths
            // are fallbacks, otherwise a site with many 404s can make one
            // candidate take minutes.
            let focus_words = self
                .focus
                .iter()
                .flat_map(|rule| rule.keywords.iter())
                .flat_map(|keyword| {
                    keyword
                        .split(|character: char| !character.is_ascii_alphanumeric())
                        .filter(|word| word.len() >= 4)
                        .map(matcher::stem)
                        .collect::<Vec<_>>()
                })
                .collect::<HashSet<_>>();
            let mut ranked = homepage
                .links
                .iter()
                .filter_map(|link| {
                    let host = link
                        .host_str()
                        .unwrap_or_default()
                        .trim_start_matches("www.");
                    if is_public_job_board(host) {
                        return Some((3, link));
                    }
                    if host != root_host {
                        return None;
                    }
                    let path = link.path().to_ascii_lowercase();
                    let slug = PAGE_SLUGS.iter().any(|part| path.contains(part));
                    let relevance = path
                        .split(|character: char| !character.is_ascii_alphanumeric())
                        .filter(|word| focus_words.contains(&matcher::stem(word)))
                        .count();
                    (slug || relevance > 0).then_some((usize::from(slug) + relevance * 2, link))
                })
                .collect::<Vec<_>>();
            // Stable: equally relevant links keep the order the site lists them.
            ranked.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
            urls.extend(ranked.into_iter().map(|(_, link)| link.clone()));
            for path in ["/contact", "/about", "/careers", "/news"] {
                if let Ok(url) = root.join(path) {
                    urls.push(url);
                }
            }
        }
        let mut seen = HashSet::new();
        let mut detected = HashSet::new();
        let mut results = Vec::new();
        let mut attempted = 0;
        for url in urls {
            let canonical = format!(
                "{}://{}{}",
                url.scheme(),
                url.host_str().unwrap_or_default(),
                url.path().trim_end_matches('/')
            );
            if !seen.insert(canonical) || attempted >= self.page_budget {
                continue;
            }
            let page = if url == root {
                homepage.clone()
            } else {
                if self.remaining().is_none() {
                    break;
                }
                let page_host = url
                    .host_str()
                    .unwrap_or_default()
                    .trim_start_matches("www.");
                let allowed = if page_host == root_host {
                    robots.is_empty() || robots_allows(&robots, url.path())
                } else {
                    let external = self.robots_for(&url).await;
                    external.is_empty() || robots_allows(&external, url.path())
                };
                if !allowed {
                    continue;
                }
                attempted += 1;
                match self.page(&account.id, &url).await {
                    Ok((page, fetched)) => {
                        if fetched {
                            tokio::time::sleep(Duration::from_millis(150)).await;
                        }
                        page
                    }
                    Err(_) => continue,
                }
            };
            if url == root {
                attempted += 1;
            }
            for id in &page.technologies {
                if let Some(technology) = crate::technology::find(id) {
                    if detected.insert(technology.id) {
                        results.push(technology_fact(technology, &page));
                    }
                }
            }
            if page.text.trim().is_empty() {
                continue;
            }
            results.extend(extract_facts(account, page, self.focus, self.personas));
        }
        if results.is_empty() {
            return Err(AppError::Message(
                "No permitted, readable public pages were found.".into(),
            ));
        }
        Ok(results)
    }
}

/// Public job boards the crawler may follow off the company's own domain.
///
/// Matched on a dot boundary. `host.ends_with("lever.co")` also accepted
/// `mylever.co` and `notlever.co`, registrable domains, not subdomains, so a
/// link on a crawled page could steer the crawler to an attacker-controlled
/// host, and the fetched content was then stored as `careers_page` evidence for
/// that account. That source kind is trusted by signal scoring, so it could be
/// used to manufacture hiring signals for a prospect.
fn is_public_job_board(host: &str) -> bool {
    const BOARDS: [&str; 2] = ["greenhouse.io", "lever.co"];
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    BOARDS
        .iter()
        .any(|board| host == *board || host.ends_with(&format!(".{board}")))
}

/// Facts from one page: the page itself, then one quoted line per approved rule
/// the page supports, and one per persona it names.
///
/// The line-level excerpts are what scoring credits. They are chosen with the
/// same matcher the scorer uses, from the whole page text rather than its
/// first 500 characters, so a rule can only be credited for a line that
/// actually affirms it.
pub(crate) fn extract_facts(
    account: &Account,
    page: FetchResult,
    focus: &[QualificationRule],
    personas: &[String],
) -> Vec<ResearchFact> {
    let lines = page
        .text
        .lines()
        .map(str::trim)
        .filter(|line| line.len() >= 12)
        .map(|line| (line, Text::new(line)))
        .collect::<Vec<_>>();
    let mut facts = vec![extract_fact(account, page.clone())];
    let line_fact =
        |source_kind: &str, title: String, excerpt: String, signal: &str| ResearchFact {
            source_url: page.final_url.to_string(),
            source_kind: source_kind.into(),
            title,
            excerpt: excerpt.clone(),
            confidence: "high".into(),
            etag: page.etag.clone(),
            modified_at: page.modified_at.clone(),
            contacts: Vec::new(),
            signals: if signal.is_empty() {
                Vec::new()
            } else {
                vec![(signal.into(), excerpt, "high".into())]
            },
        };
    // Each rule takes the first supporting line no earlier rule claimed, so a
    // page with distinct evidence for two rules yields two distinct quotes
    // rather than one line credited once.
    let mut claimed = HashSet::new();
    for rule in focus {
        let supporting = lines
            .iter()
            .enumerate()
            .filter(|(_, (_, text))| {
                rule.keywords
                    .iter()
                    .any(|keyword| matches_keyword(text, keyword))
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let Some(index) = supporting
            .iter()
            .find(|index| !claimed.contains(*index))
            .or(supporting.first())
            .copied()
        else {
            continue;
        };
        claimed.insert(index);
        facts.push(line_fact(
            "rule_excerpt",
            rule.label.clone(),
            excerpt_around(&lines, index),
            // Only hiring is a reason to reach out now. Other rule excerpts are
            // evidence for their rule and are shown there; repeating them as
            // uncategorised "signals" listed one sentence several times.
            if rule.id.contains("hiring") {
                "hiring"
            } else {
                ""
            },
        ));
    }
    if is_people_page(&page.final_url) {
        // A persona inside the company's own name ("Westside Talent Partners")
        // cannot be told apart from a person holding that role.
        let name = Text::new(&account.name);
        for persona in personas
            .iter()
            .filter(|value| !value.trim().is_empty() && !name.mentions(value))
        {
            if let Some(index) = lines.iter().position(|(_, text)| text.mentions(persona)) {
                facts.push(line_fact(
                    "persona_excerpt",
                    format!("Names the role: {}", persona.trim()),
                    excerpt_around(&lines, index),
                    "",
                ));
            }
        }
    }
    facts
}

/// Evidence that the business's site runs a known technology.
fn technology_fact(technology: &crate::technology::Technology, page: &FetchResult) -> ResearchFact {
    ResearchFact {
        source_url: page.final_url.to_string(),
        source_kind: "technology_detected".into(),
        title: format!("Website technology: {}", technology.name),
        excerpt: crate::technology::describe(technology, &page.final_url),
        confidence: "high".into(),
        etag: page.etag.clone(),
        modified_at: page.modified_at.clone(),
        contacts: Vec::new(),
        signals: vec![(
            "technical".into(),
            format!("Uses {} ({})", technology.name, technology.category),
            "high".into(),
        )],
    }
}

/// The matched line, with the following line appended when the match alone is
/// too short to read as evidence.
fn excerpt_around(lines: &[(&str, Text)], index: usize) -> String {
    let mut excerpt = lines[index].0.to_string();
    if excerpt.chars().count() < 48 {
        if let Some((next, _)) = lines.get(index + 1) {
            excerpt.push(' ');
            excerpt.push_str(next);
        }
    }
    excerpt.chars().take(500).collect()
}

/// Pages where a business introduces its people.
fn is_people_page(url: &Url) -> bool {
    let path = url.path().to_ascii_lowercase();
    path == "/"
        || [
            "about",
            "team",
            "leader",
            "staff",
            "people",
            "our-story",
            "who-we-are",
        ]
        .iter()
        .any(|part| path.contains(part))
}

fn extract_fact(account: &Account, page: FetchResult) -> ResearchFact {
    let source_kind = source_kind(&page.final_url);
    let excerpt = evidence_excerpt(&page.text);
    let lower = format!("{} {}", page.title, excerpt).to_ascii_lowercase();
    let mut signals = Vec::new();
    for (category, keywords) in [
        (
            "funding",
            &[
                "funding",
                "raised",
                "series a",
                "series b",
                "venture capital",
            ][..],
        ),
        (
            "contract",
            &["awarded", "contract", "procurement", "selected by"][..],
        ),
        (
            "leadership",
            &["appointed", "joins as", "new chief", "leadership team"][..],
        ),
        (
            "hiring",
            &[
                "we're hiring",
                "we are hiring",
                "open positions",
                "open roles",
                "job openings",
            ][..],
        ),
        (
            "product",
            &["launched", "introducing", "new product", "release"][..],
        ),
        (
            "partnership",
            &["partnership", "partnered with", "strategic alliance"][..],
        ),
        ("filing", &["10-k", "10-q", "8-k", "sec filing"][..]),
        (
            "technical",
            &["open source", "github", "developer platform", "api release"][..],
        ),
    ] {
        if keywords.iter().any(|keyword| lower.contains(keyword)) {
            signals.push((category.to_string(), excerpt.clone(), "medium".to_string()));
        }
    }

    let mut contacts = Vec::new();
    let domain = account.domain.as_deref().unwrap_or_default();
    // Both patterns are compiled once; this function runs for every page crawled.
    static EMAIL_PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\b[A-Z0-9._%+\-]+@[A-Z0-9.\-]+\.[A-Z]{2,}\b").unwrap());
    let email_pattern = &*EMAIL_PATTERN;
    for found in email_pattern.find_iter(&page.text) {
        let email = found
            .as_str()
            .trim_matches(|character: char| ".,;:()[]<>".contains(character))
            .to_ascii_lowercase();
        let email_domain = email.split('@').nth(1).unwrap_or_default();
        if !domain.is_empty()
            && (email_domain == domain || email_domain.ends_with(&format!(".{domain}")))
        {
            let local = email.split('@').next().unwrap_or_default();
            let kind = if matches!(
                local,
                "info" | "sales" | "hello" | "contact" | "support" | "business"
            ) {
                "role_email"
            } else {
                "work_email"
            };
            contacts.push((kind.into(), email, "high".into()));
        }
    }
    if matches!(source_kind.as_str(), "contact_page" | "company_page")
        && page
            .final_url
            .path()
            .to_ascii_lowercase()
            .contains("contact")
    {
        static PHONE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(
                r"(?:\+?1[ .\-]?)?(?:\([2-9][0-9]{2}\)|[2-9][0-9]{2})[ .\-][0-9]{3}[ .\-][0-9]{4}",
            )
            .unwrap()
        });
        let phone_pattern = &*PHONE_PATTERN;
        for line in page.text.lines() {
            let context = line.to_ascii_lowercase();
            let labeled_business_line = [
                "office",
                "main line",
                "business",
                "corporate",
                "company phone",
                "switchboard",
            ]
            .iter()
            .any(|marker| context.contains(marker));
            let labeled_personal_line = ["mobile", "cell", "text me", "personal"]
                .iter()
                .any(|marker| context.contains(marker));
            if labeled_business_line && !labeled_personal_line {
                for found in phone_pattern.find_iter(line) {
                    if let Some(phone) = db::normalize_us_phone(found.as_str()) {
                        contacts.push(("company_main".into(), phone, "medium".into()));
                    }
                }
            }
        }
    }
    contacts.sort();
    contacts.dedup();
    ResearchFact {
        source_url: page.final_url.to_string(),
        source_kind,
        title: page.title,
        excerpt,
        confidence: "medium".into(),
        etag: page.etag,
        modified_at: page.modified_at,
        contacts,
        signals,
    }
}

fn source_kind(url: &Url) -> String {
    let path = url.path().to_ascii_lowercase();
    let host = url.host_str().unwrap_or_default();
    if host == "sec.gov" || host.ends_with(".sec.gov") {
        "sec_record"
    } else if host == "usaspending.gov" || host.ends_with(".usaspending.gov") {
        "usaspending_record"
    } else if path.contains("career")
        || path.contains("jobs")
        || url.host_str().is_some_and(is_public_job_board)
    {
        "careers_page"
    } else if path.contains("contact") {
        "contact_page"
    } else if path.contains("news")
        || path.contains("press")
        || path.contains("blog")
        || path.contains("feed")
    {
        "company_news"
    } else if path.contains("team") || path.contains("leader") {
        "leadership_page"
    } else {
        "company_page"
    }
    .into()
}

fn evidence_excerpt(text: &str) -> String {
    let mut excerpt = String::new();
    for line in text.lines().filter(|line| line.trim().len() >= 20) {
        if !excerpt.is_empty() {
            excerpt.push(' ');
        }
        excerpt.push_str(line.trim());
        if excerpt.chars().count() >= 480 {
            break;
        }
    }
    excerpt.chars().take(500).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Account;

    fn account() -> Account {
        Account {
            id: "a".into(),
            name: "Acme".into(),
            domain: Some("acme.example".into()),
            website_url: None,
            industry: None,
            employee_band: None,
            location: None,
            status: "new".into(),
            score: None,
            score_label: None,
            researched_at: None,
            created_at: "now".into(),
            saved_at: Some("now".into()),
            favorited_at: None,
            trashed_at: None,
        }
    }

    #[test]
    fn keeps_only_same_domain_business_email() {
        let page = FetchResult { final_url: Url::parse("https://acme.example/contact").unwrap(), title: "Contact".into(), text: "Contact sales@acme.example or personal@gmail.com.\nCall (212) 555-0100 for our business office.".into(), links: vec![], etag: None, modified_at: None, technologies: vec![] };
        let fact = extract_fact(&account(), page);
        assert!(fact
            .contacts
            .iter()
            .any(|(_, value, _)| value == "sales@acme.example"));
        assert!(!fact
            .contacts
            .iter()
            .any(|(_, value, _)| value.contains("gmail")));
        assert!(fact
            .contacts
            .iter()
            .any(|(_, value, _)| value == "+12125550100"));
    }

    #[test]
    fn rejects_ambiguous_or_mobile_phone_numbers() {
        let page = FetchResult {
            final_url: Url::parse("https://acme.example/contact").unwrap(),
            title: "Contact".into(),
            text: "Call (212) 555-0100.\nMobile office: (212) 555-0199.".into(),
            links: vec![],
            etag: None,
            modified_at: None,
            technologies: vec![],
        };
        let fact = extract_fact(&account(), page);
        assert!(!fact
            .contacts
            .iter()
            .any(|(kind, _, _)| kind == "company_main"));
    }

    #[test]
    fn the_job_board_allowlist_is_anchored_on_a_dot_boundary() {
        // The crawler may only leave the company's own domain for these hosts.
        // `ends_with("lever.co")` also accepted registrable lookalikes, so a
        // link on a crawled page could send the crawler to an attacker's host
        // and have the result stored as trusted `careers_page` evidence.
        for allowed in [
            "greenhouse.io",
            "boards.greenhouse.io",
            "www.greenhouse.io",
            "lever.co",
            "jobs.lever.co",
        ] {
            assert!(is_public_job_board(allowed), "{allowed} should be allowed");
        }
        for lookalike in [
            "mylever.co",
            "notlever.co",
            "evilgreenhouse.io",
            "lever.co.attacker.test",
            "greenhouse.io.evil.test",
            "fakegreenhouse.io",
            "",
        ] {
            assert!(
                !is_public_job_board(lookalike),
                "{lookalike} must not pass the allowlist"
            );
        }
    }

    #[test]
    fn a_lookalike_job_board_is_not_classified_as_a_careers_page() {
        // `careers_page` is trusted by signal scoring, so misclassifying an
        // attacker-controlled host would let it manufacture hiring signals.
        assert_eq!(
            source_kind(&Url::parse("https://jobs.lever.co/acme/1").unwrap()),
            "careers_page"
        );
        assert_eq!(
            source_kind(&Url::parse("https://mylever.co/acme").unwrap()),
            "company_page"
        );
        assert_eq!(
            source_kind(&Url::parse("https://evilgreenhouse.io/x").unwrap()),
            "company_page"
        );
    }

    #[test]
    fn creates_separate_focused_workflow_evidence() {
        let page = FetchResult {
            final_url: Url::parse("https://acme.example/services").unwrap(),
            title: "Services".into(),
            text: "Acme provides routine maintenance for local customers.\nCall now for 24 hour emergency plumbing service.\nWe are hiring a dispatcher for our expanding service team.".into(),
            links: vec![],
            etag: None,
            modified_at: None,
            technologies: vec![],
        };
        let facts = extract_facts(
            &account(),
            page,
            &crate::signals::default_focus(),
            &["Owner".into()],
        );
        let excerpt_for = |label: &str| {
            facts
                .iter()
                .find(|item| item.source_kind == "rule_excerpt" && item.title == label)
                .map(|item| item.excerpt.clone())
        };
        assert_eq!(
            excerpt_for("After-hours or emergency service").as_deref(),
            Some("Call now for 24 hour emergency plumbing service.")
        );
        assert!(excerpt_for("Phone-first customer intake").is_some());
        let hiring = facts
            .iter()
            .find(|item| item.title == "Intake, dispatch, or receptionist hiring")
            .expect("the dispatcher line is hiring evidence");
        assert!(hiring.excerpt.contains("dispatcher"));
        assert_eq!(hiring.signals[0].0, "hiring");
        // A services page is not a people page, so no persona evidence.
        assert!(facts
            .iter()
            .all(|item| item.source_kind != "persona_excerpt"));
        assert!(facts.iter().all(|item| item.excerpt.chars().count() <= 500));
    }

    #[test]
    fn rule_evidence_comes_from_the_whole_page_and_respects_negation() {
        let filler =
            "Family recipes, fresh ingredients and a warm room every single day.\n".repeat(12);
        let page = FetchResult {
            final_url: Url::parse("https://acme.example/about").unwrap(),
            title: "About".into(),
            text: format!(
                "{filler}We are not hiring at this time.\nMeet the owner, Maria Lopez, who opened Acme in 1998.\nNow hiring servers and line cooks for weekend shifts."
            ),
            links: vec![],
            etag: None,
            modified_at: None,
            technologies: vec![],
        };
        let rule = crate::signals::rule_from_template(
            crate::signals::template("hourly_hiring").unwrap(),
            "pack",
        );
        let facts = extract_facts(&account(), page, &[rule], &["Owner".into()]);
        let hiring = facts
            .iter()
            .find(|item| item.source_kind == "rule_excerpt")
            .expect("a line far past the first 500 characters is still found");
        assert!(hiring.excerpt.starts_with("Now hiring servers"));
        let persona = facts
            .iter()
            .find(|item| item.source_kind == "persona_excerpt")
            .expect("an about page naming the owner is persona evidence");
        assert!(persona.excerpt.contains("Maria Lopez"));
    }

    mod crawl {
        use super::super::*;
        use super::account;
        use std::collections::HashMap;
        use std::sync::Mutex;

        /// Serves fixed pages with optional delays and counts every fetch.
        struct Fixture {
            pages: HashMap<String, (u64, String)>,
            fetched: Mutex<Vec<String>>,
        }

        impl Fixture {
            fn new(pages: &[(&str, u64, &str)]) -> Self {
                Self {
                    pages: pages
                        .iter()
                        .map(|(url, delay, text)| (url.to_string(), (*delay, text.to_string())))
                        .collect(),
                    fetched: Mutex::new(Vec::new()),
                }
            }
            fn fetched(&self) -> Vec<String> {
                self.fetched.lock().unwrap().clone()
            }
        }

        #[async_trait]
        impl Fetcher for Fixture {
            async fn fetch(&self, url: Url) -> AppResult<FetchResult> {
                self.fetched.lock().unwrap().push(url.to_string());
                let Some((delay, text)) = self.pages.get(url.as_str()) else {
                    return Err(AppError::Message("not found".into()));
                };
                tokio::time::sleep(Duration::from_millis(*delay)).await;
                Ok(FetchResult {
                    final_url: url.clone(),
                    title: "Acme".into(),
                    text: text.clone(),
                    links: vec![Url::parse("https://acme.example/about").unwrap()],
                    etag: None,
                    modified_at: None,
                    // Stands in for markup detection, which has its own tests.
                    technologies: if text.contains("Book with us") {
                        vec!["calendly".into()]
                    } else {
                        vec![]
                    },
                })
            }
        }

        #[derive(Default)]
        struct Memory {
            robots: Mutex<HashMap<String, String>>,
            pages: Mutex<HashMap<String, FetchResult>>,
        }

        impl PageCache for Memory {
            fn robots(&self, host: &str) -> Option<String> {
                self.robots.lock().unwrap().get(host).cloned()
            }
            fn store_robots(&self, host: &str, text: &str) {
                self.robots.lock().unwrap().insert(host.into(), text.into());
            }
            fn page(&self, _: &str, url: &Url) -> Option<FetchResult> {
                self.pages.lock().unwrap().get(url.as_str()).cloned()
            }
            fn store_page(&self, _: &str, url: &Url, page: &FetchResult) {
                self.pages
                    .lock()
                    .unwrap()
                    .insert(url.to_string(), page.clone());
            }
        }

        fn site() -> Account {
            let mut account = account();
            account.website_url = Some("https://acme.example/".into());
            account
        }

        const HOME: &str = "Acme provides 24/7 emergency plumbing across the city.";

        #[tokio::test]
        async fn a_passed_deadline_keeps_the_pages_already_read() {
            let fixture = Fixture::new(&[
                ("https://acme.example/", 0, HOME),
                (
                    "https://acme.example/about",
                    2_000,
                    "About Acme and its long history here.",
                ),
            ]);
            let facts = Crawl {
                fetcher: &fixture,
                cache: &NoCache,
                focus: &[],
                personas: &[],
                page_budget: 3,
                deadline: Instant::now() + Duration::from_millis(300),
            }
            .run(&site())
            .await
            .expect("the homepage arrived in time, so its facts survive");
            assert!(facts.iter().any(|fact| fact.excerpt.contains("emergency")));
            assert!(facts.iter().all(|fact| !fact.source_url.contains("about")));
        }

        #[tokio::test]
        async fn detected_technologies_become_one_evidence_row_each() {
            let fixture = Fixture::new(&[
                (
                    "https://acme.example/",
                    0,
                    "Book with us for 24/7 plumbing anywhere in town.",
                ),
                (
                    "https://acme.example/about",
                    0,
                    "Book with us. About Acme and its long history.",
                ),
            ]);
            let facts = Crawl {
                fetcher: &fixture,
                cache: &NoCache,
                focus: &[],
                personas: &[],
                page_budget: 3,
                deadline: Instant::now() + Duration::from_secs(5),
            }
            .run(&site())
            .await
            .unwrap();
            let detected = facts
                .iter()
                .filter(|fact| fact.source_kind == "technology_detected")
                .collect::<Vec<_>>();
            assert_eq!(detected.len(), 1, "two pages, one technology, one row");
            assert_eq!(detected[0].title, "Website technology: Calendly");
        }

        #[tokio::test]
        async fn cached_robots_and_pages_skip_the_network() {
            let fixture = Fixture::new(&[
                (
                    "https://acme.example/robots.txt",
                    0,
                    "User-agent: *\nAllow: /",
                ),
                ("https://acme.example/", 0, HOME),
            ]);
            let cache = Memory::default();
            let crawl = Crawl {
                fetcher: &fixture,
                cache: &cache,
                focus: &[],
                personas: &[],
                page_budget: 1,
                deadline: Instant::now() + Duration::from_secs(5),
            };
            crawl.run(&site()).await.unwrap();
            let first = fixture.fetched();
            assert!(first.iter().any(|url| url.ends_with("/robots.txt")));
            crawl.run(&site()).await.unwrap();
            assert_eq!(fixture.fetched(), first, "the second crawl fetched nothing");
        }

        #[tokio::test]
        async fn a_disallowed_homepage_is_never_fetched() {
            let fixture = Fixture::new(&[
                (
                    "https://acme.example/robots.txt",
                    0,
                    "User-agent: *\nDisallow: /",
                ),
                ("https://acme.example/", 0, HOME),
            ]);
            let result = Crawl {
                fetcher: &fixture,
                cache: &NoCache,
                focus: &[],
                personas: &[],
                page_budget: 3,
                deadline: Instant::now() + Duration::from_secs(5),
            }
            .run(&site())
            .await;
            assert!(result.is_err());
            assert_eq!(fixture.fetched(), vec!["https://acme.example/robots.txt"]);
        }

        #[tokio::test]
        async fn a_slow_robots_file_does_not_consume_the_budget() {
            let fixture = Fixture::new(&[
                ("https://acme.example/robots.txt", 10_000, ""),
                ("https://acme.example/", 0, HOME),
            ]);
            let started = Instant::now();
            Crawl {
                fetcher: &fixture,
                cache: &NoCache,
                focus: &[],
                personas: &[],
                page_budget: 1,
                deadline: Instant::now() + Duration::from_secs(6),
            }
            .run(&site())
            .await
            .unwrap();
            assert!(started.elapsed() < Duration::from_secs(3));
        }
    }
}
