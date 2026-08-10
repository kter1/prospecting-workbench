use reqwest::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use url::Url;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::Account;
use crate::models::DiscoveryResult;
use crate::research::ResearchFact;
use crate::safety::{looks_like_embedded_instruction, validate_url};

const SEC_TICKERS: &str = "https://www.sec.gov/files/company_tickers.json";
const USASPENDING_RECIPIENTS: &str = "https://api.usaspending.gov/api/v2/autocomplete/recipient/";

pub async fn discover(
    client: &Client,
    configured_searxng: Option<String>,
    source: &str,
    query: &str,
) -> AppResult<Vec<DiscoveryResult>> {
    let query = query.trim();
    if query.len() < 2 {
        return Err(AppError::Message(
            "Use at least two characters for public-source search.".into(),
        ));
    }
    match source {
        "overpass" => Err(AppError::Message(
            "OpenStreetMap discovery requires an approved structured search brief.".into(),
        )),
        "sec" => discover_sec(client, query).await,
        "usaspending" => discover_usaspending(client, query).await,
        "searxng" => discover_searxng(client, configured_searxng, query).await,
        _ => Err(AppError::Message("Unknown discovery source.".into())),
    }
}

async fn discover_sec(client: &Client, query: &str) -> AppResult<Vec<DiscoveryResult>> {
    let payload = limited_json(client.get(SEC_TICKERS).send().await?)
        .await
        .ok_or_else(|| {
            AppError::Message("SEC EDGAR returned an unavailable or oversized response.".into())
        })?;
    let needle = query.to_ascii_lowercase();
    let mut results = payload
        .as_object()
        .into_iter()
        .flat_map(|object| object.values())
        .filter_map(|item| {
            let name = item.get("title")?.as_str()?;
            let ticker = item
                .get("ticker")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !name.to_ascii_lowercase().contains(&needle)
                && !ticker.to_ascii_lowercase().contains(&needle)
            {
                return None;
            }
            let cik = item.get("cik_str").and_then(Value::as_i64)?;
            let source_url =
                format!("https://www.sec.gov/edgar/browse/?CIK={cik:010}&owner=exclude");
            Some(DiscoveryResult {
                source: "SEC EDGAR".into(),
                company_name: name.into(),
                domain: None,
                website_url: Some(source_url.clone()),
                external_id: Some(format!("{cik:010}")),
                reason: format!("SEC registrant {ticker} · CIK {cik:010}"),
                source_url,
                source_external_id: Some(format!("{cik:010}")),
                business_phone: None,
                location: None,
                identity_confidence: Some("high".into()),
            })
        })
        .take(20)
        .collect::<Vec<_>>();
    results.sort_by(|a, b| a.company_name.cmp(&b.company_name));
    Ok(results)
}

async fn discover_usaspending(client: &Client, query: &str) -> AppResult<Vec<DiscoveryResult>> {
    let response = client
        .post(USASPENDING_RECIPIENTS)
        .json(&json!({ "search_text": query, "limit": 20 }))
        .send()
        .await?;
    let payload = limited_json(response).await.ok_or_else(|| {
        AppError::Message("USAspending returned an unavailable or oversized response.".into())
    })?;
    let recipients = payload
        .pointer("/results/recipients")
        .or_else(|| payload.get("results"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(recipients
        .into_iter()
        .filter_map(|item| {
            let name = item
                .get("recipient_name")
                .or_else(|| item.get("name"))?
                .as_str()?;
            let hash = item
                .get("recipient_hash")
                .or_else(|| item.get("hash"))
                .and_then(Value::as_str);
            let source_url = hash
                .map(|value| format!("https://www.usaspending.gov/recipient/{value}/latest"))
                .unwrap_or_else(|| "https://www.usaspending.gov/search".into());
            Some(DiscoveryResult {
                source: "USAspending".into(),
                company_name: name.into(),
                domain: None,
                website_url: Some(source_url.clone()),
                external_id: hash.map(str::to_owned),
                reason: "Matched a public federal-award recipient record.".into(),
                source_url,
                source_external_id: hash.map(str::to_owned),
                business_phone: None,
                location: None,
                identity_confidence: Some("high".into()),
            })
        })
        .take(20)
        .collect())
}

async fn discover_searxng(
    client: &Client,
    configured: Option<String>,
    query: &str,
) -> AppResult<Vec<DiscoveryResult>> {
    let configured = configured
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::Message("Configure a SearXNG URL in Settings first.".into()))?;
    let mut endpoint = Url::parse(&configured)?;
    // A loopback SearXNG URL is an explicit, scoped exception. It is never accepted by the crawler.
    let is_loopback = endpoint.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if !is_loopback {
        validate_url(&endpoint)?;
    }
    endpoint.set_path("/search");
    endpoint.set_query(None);
    endpoint
        .query_pairs_mut()
        .append_pair("q", query)
        .append_pair("format", "json")
        .append_pair("categories", "general");
    let payload = limited_json(client.get(endpoint).send().await?)
        .await
        .ok_or_else(|| {
            AppError::Message("SearXNG returned an unavailable or oversized response.".into())
        })?;
    let results = payload
        .get("results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(results
        .into_iter()
        .filter_map(|item| {
            let source_url = item.get("url")?.as_str()?.to_string();
            let url = Url::parse(&source_url).ok()?;
            validate_url(&url).ok()?;
            let domain = db::normalize_domain(&source_url);
            let title = item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_else(|| domain.as_deref().unwrap_or("Public result"));
            Some(DiscoveryResult {
                source: "SearXNG".into(),
                company_name: title.into(),
                domain: domain.clone(),
                website_url: Some(source_url.clone()),
                external_id: None,
                reason: item
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("Selected public search result.")
                    .chars()
                    .take(180)
                    .collect(),
                source_url,
                source_external_id: None,
                business_phone: None,
                location: None,
                identity_confidence: Some(if domain.is_some() { "medium" } else { "low" }.into()),
            })
        })
        .take(20)
        .collect())
}

pub async fn discover_overpass(
    client: &Client,
    categories: &[String],
    geography: &str,
    limit: usize,
) -> AppResult<Vec<DiscoveryResult>> {
    let filters = categories
        .iter()
        .filter_map(|category| overpass_filter(category).map(|filter| (category.as_str(), filter)))
        .collect::<Vec<_>>();
    if filters.is_empty() {
        return Ok(Vec::new());
    }
    let regions = bounded_regions(geography);
    if regions.is_empty() {
        return Err(AppError::Message(
            "Automatic map discovery is unavailable for this geography. Use a supported bounded U.S. scope or guided discovery.".into(),
        ));
    }
    // Large multi-region Overpass unions routinely queue for 15+ seconds. Query
    // one category/sample at a time and stop as soon as the candidate budget is
    // met. A default ten-result search usually needs only the first request.
    let mut results = Vec::new();
    let mut attempts = 0;
    'regions: for (region_label, bbox) in regions {
        for (category, filter) in &filters {
            if results.len() >= limit || attempts >= 3 {
                break 'regions;
            }
            attempts += 1;
            let query = format!(
                "[out:json][timeout:6];nwr{filter}({bbox});out tags {};",
                limit.clamp(5, 100)
            );
            let response = tokio::time::timeout(
                std::time::Duration::from_secs(8),
                client
                    .post("https://overpass-api.de/api/interpreter")
                    .header("Accept", "application/json")
                    .form(&[("data", query)])
                    .send(),
            )
            .await
            .map_err(|_| {
                AppError::Message(
                    "OpenStreetMap discovery exceeded its 8-second budget. Retry or use guided discovery."
                        .into(),
                )
            })??;
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let retry_after = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<i64>().ok())
                    .unwrap_or(60)
                    .clamp(1, 86_400);
                return Err(AppError::Message(
                    format!("RATE_LIMIT:overpass:{retry_after}:The public Overpass instance is rate-limited. Cached or guided discovery can be used while it recovers."),
                ));
            }
            let payload = limited_json(response).await.ok_or_else(|| {
                AppError::Message(
                    "OpenStreetMap Overpass returned an unavailable or oversized response.".into(),
                )
            })?;
            let elements = payload
                .get("elements")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for element in elements {
                let tags = match element.get("tags").and_then(Value::as_object) {
                    Some(tags) => tags,
                    None => continue,
                };
                let name = match tags.get("name").and_then(Value::as_str).map(str::trim) {
                    Some(name) if !name.is_empty() && !looks_like_embedded_instruction(name) => {
                        name
                    }
                    _ => continue,
                };
                let element_type = element
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("node");
                let id = match element.get("id").and_then(Value::as_i64) {
                    Some(id) => id,
                    None => continue,
                };
                let website_url = tags
                    .get("contact:website")
                    .or_else(|| tags.get("website"))
                    .and_then(Value::as_str)
                    .and_then(db::normalize_public_url);
                let domain = website_url.as_deref().and_then(db::normalize_domain);
                let phone = tags
                    .get("contact:phone")
                    .or_else(|| tags.get("phone"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let city = tags.get("addr:city").and_then(Value::as_str);
                let state = tags.get("addr:state").and_then(Value::as_str);
                let location = match (city, state) {
                    (Some(city), Some(state)) => Some(format!("{city}, {state}, United States")),
                    (Some(city), None) => Some(format!("{city}, United States")),
                    (None, Some(state)) => Some(format!("{state}, United States")),
                    (None, None) => None,
                };
                let matched_category = category.replace('_', " ");
                let source_url = format!("https://www.openstreetmap.org/{element_type}/{id}");
                results.push(DiscoveryResult {
                    source: "OpenStreetMap".into(),
                    company_name: name.into(),
                    domain,
                    website_url,
                    external_id: Some(format!("{element_type}/{id}")),
                    reason: format!(
                        "Mapped {matched_category} in the bounded {region_label} sample."
                    ),
                    source_url,
                    source_external_id: Some(format!("{element_type}/{id}")),
                    business_phone: phone,
                    location,
                    identity_confidence: Some("medium".into()),
                });
            }
        }
    }
    results.sort_by(|a, b| a.company_name.cmp(&b.company_name));
    results.dedup_by(|a, b| {
        a.domain.is_some() && a.domain == b.domain
            || (a.company_name.eq_ignore_ascii_case(&b.company_name) && a.location == b.location)
    });
    results.truncate(limit);
    Ok(results)
}

fn overpass_filter(category: &str) -> Option<&'static str> {
    match category {
        "plumber" => Some("[\"craft\"=\"plumber\"]"),
        "electrician" => Some("[\"craft\"=\"electrician\"]"),
        "hvac" => Some("[\"craft\"=\"hvac\"]"),
        "roofer" => Some("[\"craft\"=\"roofer\"]"),
        "lawyer" => Some("[\"office\"=\"lawyer\"]"),
        "property_management" => Some("[\"office\"=\"property_management\"]"),
        "car_repair" => Some("[\"shop\"=\"car_repair\"]"),
        "restaurant" => Some("[\"amenity\"=\"restaurant\"]"),
        "hotel" => Some("[\"tourism\"=\"hotel\"]"),
        _ => None,
    }
}

fn bounded_regions(geography: &str) -> Vec<(&'static str, &'static str)> {
    let normalized = geography.trim().to_ascii_lowercase();
    if normalized.contains("pennsylvania") {
        return vec![("Pennsylvania", "39.7198,-80.5199,42.2699,-74.6895")];
    }
    if normalized.contains("new york") {
        return vec![("New York", "40.4774,-79.7624,45.0159,-71.7517")];
    }
    if normalized.contains("california") {
        return vec![("California", "32.5343,-124.4096,42.0095,-114.1308")];
    }
    if normalized.contains("texas") {
        return vec![("Texas", "25.8371,-106.6456,36.5007,-93.5083")];
    }
    if normalized.contains("florida") {
        return vec![("Florida", "24.3963,-87.6349,31.0009,-79.9743")];
    }
    if normalized.contains("united states") || normalized == "us" || normalized == "u.s." {
        return vec![
            ("Philadelphia metro", "39.78,-75.55,40.18,-74.85"),
            ("Chicago metro", "41.55,-88.35,42.15,-87.35"),
            ("Atlanta metro", "33.45,-84.85,34.20,-83.85"),
            ("Denver metro", "39.45,-105.35,40.05,-104.55"),
        ];
    }
    Vec::new()
}

pub fn get_searxng_url(connection: &Connection) -> AppResult<String> {
    Ok(connection
        .query_row(
            "SELECT value FROM settings WHERE key='searxng_url'",
            [],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_default())
}

pub fn set_searxng_url(connection: &Connection, value: &str) -> AppResult<String> {
    let value = value.trim().trim_end_matches('/');
    if value.is_empty() {
        connection.execute("DELETE FROM settings WHERE key='searxng_url'", [])?;
        return Ok(String::new());
    }
    let parsed = Url::parse(value)?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(AppError::Message(
            "SearXNG must be an HTTP or HTTPS base URL.".into(),
        ));
    }
    connection.execute(
        "INSERT INTO settings(key,value) VALUES('searxng_url',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![value],
    )?;
    Ok(value.into())
}

pub async fn refresh_public_signals(client: &Client, account: &Account) -> Vec<ResearchFact> {
    let mut facts = Vec::new();
    if let Some(domain) = account.domain.as_deref() {
        if let Some(fact) = refresh_gdelt(client, domain).await {
            facts.extend(fact);
        }
        if let Some(fact) = refresh_github(client, account, domain).await {
            facts.extend(fact);
        }
    }
    facts
}

async fn refresh_gdelt(client: &Client, domain: &str) -> Option<Vec<ResearchFact>> {
    let mut endpoint = Url::parse("https://api.gdeltproject.org/api/v2/doc/doc").ok()?;
    endpoint
        .query_pairs_mut()
        .append_pair("query", &format!("domain:{domain}"))
        .append_pair("mode", "artlist")
        .append_pair("maxrecords", "5")
        .append_pair("format", "json")
        .append_pair("sort", "datedesc");
    let payload = limited_json(client.get(endpoint).send().await.ok()?).await?;
    let articles = payload.get("articles")?.as_array()?;
    Some(
        articles
            .iter()
            .filter_map(|article| {
                let title = article.get("title")?.as_str()?.trim();
                let source_url = article.get("url")?.as_str()?;
                let parsed = Url::parse(source_url).ok()?;
                validate_url(&parsed).ok()?;
                if title.is_empty() || looks_like_embedded_instruction(title) {
                    return None;
                }
                Some(ResearchFact {
                    source_url: source_url.into(),
                    source_kind: "gdelt_news".into(),
                    title: title.chars().take(180).collect(),
                    excerpt: format!(
                        "GDELT indexed recent public coverage titled: {}",
                        title.chars().take(420).collect::<String>()
                    ),
                    confidence: "medium".into(),
                    etag: None,
                    modified_at: None,
                    contacts: Vec::new(),
                    signals: vec![(
                        "other".into(),
                        format!(
                            "Recent public coverage: {}",
                            title.chars().take(400).collect::<String>()
                        ),
                        "medium".into(),
                    )],
                })
            })
            .collect(),
    )
}

async fn refresh_github(
    client: &Client,
    account: &Account,
    domain: &str,
) -> Option<Vec<ResearchFact>> {
    let stem = domain
        .split('.')
        .next()
        .unwrap_or_default()
        .replace('-', "");
    let query = format!("{} in:login type:org", stem);
    let mut endpoint = Url::parse("https://api.github.com/search/users").ok()?;
    endpoint
        .query_pairs_mut()
        .append_pair("q", &query)
        .append_pair("per_page", "5");
    let payload = limited_json(
        client
            .get(endpoint)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .ok()?,
    )
    .await?;
    let normalized_name: String = account
        .name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    let matches = payload.get("items")?.as_array()?.iter().filter_map(|item| {
        let login = item.get("login")?.as_str()?;
        let normalized_login: String = login.chars().filter(|character| character.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        if normalized_login != stem && normalized_login != normalized_name {
            return None;
        }
        let source_url = item.get("html_url")?.as_str()?;
        Some(ResearchFact {
            source_url: source_url.into(),
            source_kind: "github_org".into(),
            title: format!("GitHub organization: {login}"),
            excerpt: format!("A public GitHub organization matching the account identity is available at {source_url}."),
            confidence: "medium".into(),
            etag: None,
            modified_at: None,
            contacts: Vec::new(),
            signals: vec![("technical".into(), format!("Public technical activity is visible for the matching GitHub organization {login}."), "medium".into())],
        })
    }).collect::<Vec<_>>();
    (!matches.is_empty()).then_some(matches)
}

async fn limited_json(response: reqwest::Response) -> Option<Value> {
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length > 2 * 1024 * 1024)
    {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    if bytes.len() > 2 * 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overpass_uses_only_curated_categories_and_bounded_regions() {
        assert_eq!(overpass_filter("plumber"), Some("[\"craft\"=\"plumber\"]"));
        assert!(overpass_filter("[out:xml];node[*]").is_none());
        assert_eq!(bounded_regions("United States").len(), 4);
        assert_eq!(bounded_regions("Pennsylvania").len(), 1);
        assert!(bounded_regions("Everywhere on earth").is_empty());
    }
}
