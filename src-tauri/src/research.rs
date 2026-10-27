use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use reqwest::Client;
use url::Url;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::Account;
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

pub async fn crawl_account(client: &Client, account: &Account) -> AppResult<Vec<ResearchFact>> {
    crawl_account_with_page_budget(client, account, 8).await
}

pub async fn crawl_account_quick(
    client: &Client,
    account: &Account,
) -> AppResult<Vec<ResearchFact>> {
    crawl_account_with_page_budget(client, account, 3).await
}

async fn crawl_account_with_page_budget(
    client: &Client,
    account: &Account,
    page_budget: usize,
) -> AppResult<Vec<ResearchFact>> {
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
    let robots_url = root.join("/robots.txt")?;
    let robots = match fetch_public(client, robots_url).await {
        Ok(result) => result.text,
        Err(_) => String::new(),
    };

    let homepage = fetch_public(client, root.clone()).await?;
    let mut urls = vec![root.clone()];
    let root_host = root
        .host_str()
        .unwrap_or_default()
        .trim_start_matches("www.");
    if account.domain.is_some() {
        // Prefer links the company actually publishes. Guessed paths are fallbacks,
        // otherwise a site with many 404s can make one candidate take minutes.
        for link in &homepage.links {
            let host = link
                .host_str()
                .unwrap_or_default()
                .trim_start_matches("www.");
            let lower = link.path().to_ascii_lowercase();
            let is_public_job_board = is_public_job_board(host);
            if (host == root_host
                && [
                    "about", "contact", "team", "leader", "career", "jobs", "news", "press", "blog",
                ]
                .iter()
                .any(|part| lower.contains(part)))
                || is_public_job_board
            {
                urls.push(link.clone());
            }
        }
        for path in ["/contact", "/about", "/careers", "/news"] {
            if let Ok(url) = root.join(path) {
                urls.push(url);
            }
        }
    }
    let mut seen = HashSet::new();
    let mut results = Vec::new();
    let mut attempted = 0;
    for url in urls {
        let canonical = format!(
            "{}://{}{}",
            url.scheme(),
            url.host_str().unwrap_or_default(),
            url.path().trim_end_matches('/')
        );
        if !seen.insert(canonical) || attempted >= page_budget {
            continue;
        }
        let page_host = url
            .host_str()
            .unwrap_or_default()
            .trim_start_matches("www.");
        let allowed = if page_host == root_host {
            robots.is_empty() || robots_allows(&robots, url.path())
        } else {
            let external_robots = url.join("/robots.txt").ok();
            match external_robots {
                Some(robots_url) => fetch_public(client, robots_url)
                    .await
                    .map(|result| robots_allows(&result.text, url.path()))
                    .unwrap_or(true),
                None => true,
            }
        };
        if !allowed {
            continue;
        }
        attempted += 1;
        let page = if url == root {
            homepage.clone()
        } else {
            match fetch_public(client, url).await {
                Ok(page) => page,
                Err(_) => continue,
            }
        };
        if page.text.trim().is_empty() {
            continue;
        }
        results.extend(extract_facts(account, page));
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    if results.is_empty() {
        return Err(AppError::Message(
            "No permitted, readable public pages were found.".into(),
        ));
    }
    Ok(results)
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

pub(crate) fn extract_facts(account: &Account, page: FetchResult) -> Vec<ResearchFact> {
    let mut facts = vec![extract_fact(account, page.clone())];
    let groups: [(&str, &str, &[&str]); 5] = [
        (
            "workflow_after_hours",
            "After-hours service evidence",
            &["24/7", "24 hour", "after hours", "emergency service"],
        ),
        (
            "workflow_call_first",
            "Phone-first intake evidence",
            &["call now", "call us", "speak with", "phone consultation"],
        ),
        (
            "workflow_appointment",
            "Appointment or consultation evidence",
            &[
                "schedule an appointment",
                "book by phone",
                "consultation",
                "request service",
            ],
        ),
        (
            "workflow_hiring",
            "Intake or dispatch hiring evidence",
            &[
                "intake specialist",
                "dispatcher",
                "receptionist",
                "customer service representative",
            ],
        ),
        (
            "workflow_locations",
            "Multiple-location evidence",
            &["our locations", "service areas", "areas we serve"],
        ),
    ];
    for (source_kind, title, keywords) in groups {
        if let Some(excerpt) = focused_excerpt(&page.text, keywords) {
            facts.push(ResearchFact {
                source_url: page.final_url.to_string(),
                source_kind: source_kind.into(),
                title: title.into(),
                excerpt: excerpt.clone(),
                confidence: "high".into(),
                etag: page.etag.clone(),
                modified_at: page.modified_at.clone(),
                contacts: Vec::new(),
                signals: vec![(
                    if source_kind == "workflow_hiring" {
                        "hiring"
                    } else {
                        "other"
                    }
                    .into(),
                    excerpt,
                    "high".into(),
                )],
            });
        }
    }
    facts
}

fn focused_excerpt(text: &str, keywords: &[&str]) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.len() >= 20)
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            keywords.iter().any(|keyword| lower.contains(keyword))
        })
        .map(|line| line.chars().take(500).collect())
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
        let page = FetchResult { final_url: Url::parse("https://acme.example/contact").unwrap(), title: "Contact".into(), text: "Contact sales@acme.example or personal@gmail.com.\nCall (212) 555-0100 for our business office.".into(), links: vec![], etag: None, modified_at: None };
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
        };
        let facts = extract_facts(&account(), page);
        assert!(facts
            .iter()
            .any(|item| item.source_kind == "workflow_after_hours"));
        assert!(facts
            .iter()
            .any(|item| item.source_kind == "workflow_call_first"));
        assert!(facts
            .iter()
            .any(|item| item.source_kind == "workflow_hiring"));
        assert!(facts.iter().all(|item| item.excerpt.chars().count() <= 500));
    }
}
