//! Curated U.S. bounding boxes for bounded OpenStreetMap sampling.
//!
//! Every region here is static data. The public Nominatim service is
//! deliberately not used: its usage policy forbids systematic business
//! enumeration, and geocoding a user-supplied place name would turn an
//! offline lookup into a network dependency on every search.
//!
//! `location` is the string written onto a discovered account when the OSM
//! element carries no `addr:*` tags. It is a fact we proved rather than a
//! guess: the element was returned by a query whose bounding box is inside
//! this region, so the account is inside this region.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundedRegion {
    /// Short human label used in progress copy and discovery reasons.
    pub label: &'static str,
    /// Full location string suitable for `accounts.location`.
    pub location: &'static str,
    /// Overpass bounding box: south,west,north,east.
    pub bbox: &'static str,
}

struct StateEntry {
    /// Lowercase match key.
    name: &'static str,
    /// Display name used in progress copy and discovery reasons.
    display: &'static str,
    code: &'static str,
    location: &'static str,
    bbox: &'static str,
}

struct MetroEntry {
    label: &'static str,
    location: &'static str,
    bbox: &'static str,
    aliases: &'static [&'static str],
}

#[rustfmt::skip]
const STATES: &[StateEntry] = &[
    StateEntry { name: "alabama", display: "Alabama", code: "al", location: "Alabama, United States", bbox: "30.144,-88.473,35.008,-84.889" },
    StateEntry { name: "alaska", display: "Alaska", code: "ak", location: "Alaska, United States", bbox: "54.775,-168.000,71.441,-130.000" },
    StateEntry { name: "arizona", display: "Arizona", code: "az", location: "Arizona, United States", bbox: "31.332,-114.816,37.004,-109.045" },
    StateEntry { name: "arkansas", display: "Arkansas", code: "ar", location: "Arkansas, United States", bbox: "33.004,-94.618,36.500,-89.644" },
    StateEntry { name: "california", display: "California", code: "ca", location: "California, United States", bbox: "32.534,-124.410,42.010,-114.131" },
    StateEntry { name: "colorado", display: "Colorado", code: "co", location: "Colorado, United States", bbox: "36.993,-109.060,41.003,-102.042" },
    StateEntry { name: "connecticut", display: "Connecticut", code: "ct", location: "Connecticut, United States", bbox: "40.980,-73.728,42.050,-71.787" },
    StateEntry { name: "delaware", display: "Delaware", code: "de", location: "Delaware, United States", bbox: "38.451,-75.789,39.839,-75.049" },
    StateEntry { name: "district of columbia", display: "District of Columbia", code: "dc", location: "District of Columbia, United States", bbox: "38.791,-77.120,38.996,-76.909" },
    StateEntry { name: "florida", display: "Florida", code: "fl", location: "Florida, United States", bbox: "24.396,-87.635,31.001,-79.974" },
    StateEntry { name: "georgia", display: "Georgia", code: "ga", location: "Georgia, United States", bbox: "30.356,-85.605,35.001,-80.840" },
    StateEntry { name: "hawaii", display: "Hawaii", code: "hi", location: "Hawaii, United States", bbox: "18.911,-160.247,22.235,-154.806" },
    StateEntry { name: "idaho", display: "Idaho", code: "id", location: "Idaho, United States", bbox: "41.988,-117.243,49.001,-111.044" },
    StateEntry { name: "illinois", display: "Illinois", code: "il", location: "Illinois, United States", bbox: "36.970,-91.513,42.508,-87.020" },
    StateEntry { name: "indiana", display: "Indiana", code: "in", location: "Indiana, United States", bbox: "37.771,-88.098,41.761,-84.785" },
    StateEntry { name: "iowa", display: "Iowa", code: "ia", location: "Iowa, United States", bbox: "40.375,-96.640,43.502,-90.140" },
    StateEntry { name: "kansas", display: "Kansas", code: "ks", location: "Kansas, United States", bbox: "36.993,-102.052,40.003,-94.588" },
    StateEntry { name: "kentucky", display: "Kentucky", code: "ky", location: "Kentucky, United States", bbox: "36.497,-89.571,39.148,-81.965" },
    StateEntry { name: "louisiana", display: "Louisiana", code: "la", location: "Louisiana, United States", bbox: "28.929,-94.043,33.020,-88.758" },
    StateEntry { name: "maine", display: "Maine", code: "me", location: "Maine, United States", bbox: "42.977,-71.084,47.460,-66.950" },
    StateEntry { name: "maryland", display: "Maryland", code: "md", location: "Maryland, United States", bbox: "37.911,-79.487,39.723,-75.049" },
    StateEntry { name: "massachusetts", display: "Massachusetts", code: "ma", location: "Massachusetts, United States", bbox: "41.187,-73.508,42.887,-69.858" },
    StateEntry { name: "michigan", display: "Michigan", code: "mi", location: "Michigan, United States", bbox: "41.696,-90.418,48.306,-82.413" },
    StateEntry { name: "minnesota", display: "Minnesota", code: "mn", location: "Minnesota, United States", bbox: "43.499,-97.239,49.384,-89.492" },
    StateEntry { name: "mississippi", display: "Mississippi", code: "ms", location: "Mississippi, United States", bbox: "30.174,-91.655,34.996,-88.098" },
    StateEntry { name: "missouri", display: "Missouri", code: "mo", location: "Missouri, United States", bbox: "35.996,-95.774,40.614,-89.099" },
    StateEntry { name: "montana", display: "Montana", code: "mt", location: "Montana, United States", bbox: "44.358,-116.050,49.001,-104.039" },
    StateEntry { name: "nebraska", display: "Nebraska", code: "ne", location: "Nebraska, United States", bbox: "39.999,-104.053,43.002,-95.308" },
    StateEntry { name: "nevada", display: "Nevada", code: "nv", location: "Nevada, United States", bbox: "35.002,-120.006,42.002,-114.040" },
    StateEntry { name: "new hampshire", display: "New Hampshire", code: "nh", location: "New Hampshire, United States", bbox: "42.697,-72.557,45.305,-70.610" },
    StateEntry { name: "new jersey", display: "New Jersey", code: "nj", location: "New Jersey, United States", bbox: "38.928,-75.560,41.357,-73.894" },
    StateEntry { name: "new mexico", display: "New Mexico", code: "nm", location: "New Mexico, United States", bbox: "31.332,-109.050,37.000,-103.002" },
    StateEntry { name: "new york", display: "New York", code: "ny", location: "New York, United States", bbox: "40.496,-79.763,45.016,-71.856" },
    StateEntry { name: "north carolina", display: "North Carolina", code: "nc", location: "North Carolina, United States", bbox: "33.842,-84.322,36.588,-75.460" },
    StateEntry { name: "north dakota", display: "North Dakota", code: "nd", location: "North Dakota, United States", bbox: "45.935,-104.049,49.001,-96.554" },
    StateEntry { name: "ohio", display: "Ohio", code: "oh", location: "Ohio, United States", bbox: "38.403,-84.820,41.978,-80.519" },
    StateEntry { name: "oklahoma", display: "Oklahoma", code: "ok", location: "Oklahoma, United States", bbox: "33.616,-103.002,37.002,-94.431" },
    StateEntry { name: "oregon", display: "Oregon", code: "or", location: "Oregon, United States", bbox: "41.992,-124.566,46.292,-116.463" },
    StateEntry { name: "pennsylvania", display: "Pennsylvania", code: "pa", location: "Pennsylvania, United States", bbox: "39.720,-80.520,42.270,-74.690" },
    StateEntry { name: "rhode island", display: "Rhode Island", code: "ri", location: "Rhode Island, United States", bbox: "41.146,-71.862,42.019,-71.121" },
    StateEntry { name: "south carolina", display: "South Carolina", code: "sc", location: "South Carolina, United States", bbox: "32.034,-83.354,35.216,-78.541" },
    StateEntry { name: "south dakota", display: "South Dakota", code: "sd", location: "South Dakota, United States", bbox: "42.480,-104.058,45.945,-96.436" },
    StateEntry { name: "tennessee", display: "Tennessee", code: "tn", location: "Tennessee, United States", bbox: "34.983,-90.310,36.678,-81.647" },
    StateEntry { name: "texas", display: "Texas", code: "tx", location: "Texas, United States", bbox: "25.837,-106.646,36.501,-93.508" },
    StateEntry { name: "utah", display: "Utah", code: "ut", location: "Utah, United States", bbox: "36.998,-114.052,42.002,-109.041" },
    StateEntry { name: "vermont", display: "Vermont", code: "vt", location: "Vermont, United States", bbox: "42.727,-73.438,45.017,-71.465" },
    StateEntry { name: "virginia", display: "Virginia", code: "va", location: "Virginia, United States", bbox: "36.541,-83.675,39.466,-75.242" },
    StateEntry { name: "washington", display: "Washington", code: "wa", location: "Washington, United States", bbox: "45.544,-124.849,49.002,-116.916" },
    StateEntry { name: "west virginia", display: "West Virginia", code: "wv", location: "West Virginia, United States", bbox: "37.202,-82.644,40.639,-77.719" },
    StateEntry { name: "wisconsin", display: "Wisconsin", code: "wi", location: "Wisconsin, United States", bbox: "42.492,-92.889,47.310,-86.249" },
    StateEntry { name: "wyoming", display: "Wyoming", code: "wy", location: "Wyoming, United States", bbox: "40.995,-111.056,45.006,-104.052" },
];

/// Ordered roughly by population. A nationwide search samples from the top of
/// this list, so the first regions visited are the densest business markets.
#[rustfmt::skip]
const METROS: &[MetroEntry] = &[
    MetroEntry { label: "New York metro", location: "New York metro, New York, United States", bbox: "40.49,-74.26,41.00,-73.68", aliases: &["new york city", "nyc", "manhattan", "brooklyn", "queens"] },
    MetroEntry { label: "Los Angeles metro", location: "Los Angeles metro, California, United States", bbox: "33.70,-118.67,34.34,-117.65", aliases: &["los angeles", "la county", "hollywood", "long beach"] },
    MetroEntry { label: "Chicago metro", location: "Chicago metro, Illinois, United States", bbox: "41.55,-88.35,42.15,-87.35", aliases: &["chicago", "chicagoland"] },
    MetroEntry { label: "Dallas-Fort Worth metro", location: "Dallas-Fort Worth metro, Texas, United States", bbox: "32.55,-97.55,33.05,-96.55", aliases: &["dallas", "fort worth", "dfw", "arlington tx"] },
    MetroEntry { label: "Houston metro", location: "Houston metro, Texas, United States", bbox: "29.50,-95.80,30.11,-95.01", aliases: &["houston"] },
    MetroEntry { label: "Washington DC metro", location: "Washington DC metro, District of Columbia, United States", bbox: "38.70,-77.55,39.10,-76.85", aliases: &["washington dc", "dc metro", "arlington va", "alexandria va", "bethesda"] },
    MetroEntry { label: "Philadelphia metro", location: "Philadelphia metro, Pennsylvania, United States", bbox: "39.78,-75.55,40.18,-74.85", aliases: &["philadelphia", "philly"] },
    MetroEntry { label: "Miami metro", location: "Miami metro, Florida, United States", bbox: "25.50,-80.50,26.35,-80.05", aliases: &["miami", "fort lauderdale", "south florida", "hialeah"] },
    MetroEntry { label: "Atlanta metro", location: "Atlanta metro, Georgia, United States", bbox: "33.45,-84.85,34.20,-83.85", aliases: &["atlanta"] },
    MetroEntry { label: "Boston metro", location: "Boston metro, Massachusetts, United States", bbox: "42.20,-71.30,42.55,-70.85", aliases: &["boston", "cambridge ma", "greater boston"] },
    MetroEntry { label: "Phoenix metro", location: "Phoenix metro, Arizona, United States", bbox: "33.20,-112.35,33.80,-111.60", aliases: &["phoenix", "scottsdale", "mesa", "tempe"] },
    MetroEntry { label: "San Francisco Bay Area", location: "San Francisco Bay Area, California, United States", bbox: "37.20,-122.55,38.05,-121.75", aliases: &["san francisco", "bay area", "oakland", "sf"] },
    MetroEntry { label: "Riverside-San Bernardino metro", location: "Riverside-San Bernardino metro, California, United States", bbox: "33.75,-117.60,34.20,-116.90", aliases: &["riverside", "san bernardino", "inland empire"] },
    MetroEntry { label: "Detroit metro", location: "Detroit metro, Michigan, United States", bbox: "42.10,-83.45,42.65,-82.85", aliases: &["detroit"] },
    MetroEntry { label: "Seattle metro", location: "Seattle metro, Washington, United States", bbox: "47.35,-122.45,47.80,-122.10", aliases: &["seattle", "bellevue", "puget sound"] },
    MetroEntry { label: "Minneapolis-St. Paul metro", location: "Minneapolis-St. Paul metro, Minnesota, United States", bbox: "44.80,-93.55,45.15,-92.95", aliases: &["minneapolis", "saint paul", "st paul", "twin cities"] },
    MetroEntry { label: "San Diego metro", location: "San Diego metro, California, United States", bbox: "32.53,-117.30,33.15,-116.85", aliases: &["san diego"] },
    MetroEntry { label: "Tampa metro", location: "Tampa metro, Florida, United States", bbox: "27.75,-82.65,28.20,-82.25", aliases: &["tampa", "st petersburg", "clearwater"] },
    MetroEntry { label: "Denver metro", location: "Denver metro, Colorado, United States", bbox: "39.45,-105.35,40.05,-104.55", aliases: &["denver", "aurora co", "boulder"] },
    MetroEntry { label: "St. Louis metro", location: "St. Louis metro, Missouri, United States", bbox: "38.45,-90.55,38.85,-90.10", aliases: &["st louis", "saint louis"] },
    MetroEntry { label: "Baltimore metro", location: "Baltimore metro, Maryland, United States", bbox: "39.15,-76.75,39.45,-76.40", aliases: &["baltimore"] },
    MetroEntry { label: "Charlotte metro", location: "Charlotte metro, North Carolina, United States", bbox: "35.05,-81.05,35.45,-80.60", aliases: &["charlotte"] },
    MetroEntry { label: "Orlando metro", location: "Orlando metro, Florida, United States", bbox: "28.35,-81.55,28.70,-81.15", aliases: &["orlando"] },
    MetroEntry { label: "San Antonio metro", location: "San Antonio metro, Texas, United States", bbox: "29.25,-98.75,29.70,-98.30", aliases: &["san antonio"] },
    MetroEntry { label: "Portland metro", location: "Portland metro, Oregon, United States", bbox: "45.35,-122.85,45.65,-122.45", aliases: &["portland"] },
    MetroEntry { label: "Sacramento metro", location: "Sacramento metro, California, United States", bbox: "38.40,-121.65,38.75,-121.20", aliases: &["sacramento"] },
    MetroEntry { label: "Pittsburgh metro", location: "Pittsburgh metro, Pennsylvania, United States", bbox: "40.30,-80.15,40.60,-79.75", aliases: &["pittsburgh"] },
    MetroEntry { label: "Las Vegas metro", location: "Las Vegas metro, Nevada, United States", bbox: "36.00,-115.35,36.35,-114.95", aliases: &["las vegas", "henderson nv"] },
    MetroEntry { label: "Austin metro", location: "Austin metro, Texas, United States", bbox: "30.15,-97.95,30.50,-97.55", aliases: &["austin"] },
    MetroEntry { label: "Cincinnati metro", location: "Cincinnati metro, Ohio, United States", bbox: "39.05,-84.70,39.35,-84.30", aliases: &["cincinnati"] },
    MetroEntry { label: "Kansas City metro", location: "Kansas City metro, Missouri, United States", bbox: "38.95,-94.75,39.25,-94.40", aliases: &["kansas city"] },
    MetroEntry { label: "Columbus metro", location: "Columbus metro, Ohio, United States", bbox: "39.85,-83.15,40.15,-82.80", aliases: &["columbus"] },
    MetroEntry { label: "Cleveland metro", location: "Cleveland metro, Ohio, United States", bbox: "41.35,-81.85,41.65,-81.50", aliases: &["cleveland"] },
    MetroEntry { label: "Indianapolis metro", location: "Indianapolis metro, Indiana, United States", bbox: "39.65,-86.35,39.95,-85.95", aliases: &["indianapolis", "indy"] },
    MetroEntry { label: "San Jose metro", location: "San Jose metro, California, United States", bbox: "37.20,-122.05,37.45,-121.75", aliases: &["san jose", "silicon valley", "sunnyvale", "santa clara"] },
    MetroEntry { label: "Nashville metro", location: "Nashville metro, Tennessee, United States", bbox: "36.05,-87.05,36.40,-86.60", aliases: &["nashville"] },
    MetroEntry { label: "Virginia Beach metro", location: "Virginia Beach metro, Virginia, United States", bbox: "36.70,-76.35,36.95,-75.95", aliases: &["virginia beach", "norfolk", "hampton roads"] },
    MetroEntry { label: "Providence metro", location: "Providence metro, Rhode Island, United States", bbox: "41.70,-71.55,41.90,-71.30", aliases: &["providence"] },
    MetroEntry { label: "Milwaukee metro", location: "Milwaukee metro, Wisconsin, United States", bbox: "42.90,-88.10,43.20,-87.85", aliases: &["milwaukee"] },
    MetroEntry { label: "Jacksonville metro", location: "Jacksonville metro, Florida, United States", bbox: "30.15,-81.85,30.50,-81.45", aliases: &["jacksonville"] },
    MetroEntry { label: "Oklahoma City metro", location: "Oklahoma City metro, Oklahoma, United States", bbox: "35.35,-97.70,35.65,-97.35", aliases: &["oklahoma city", "okc"] },
    MetroEntry { label: "Raleigh metro", location: "Raleigh metro, North Carolina, United States", bbox: "35.70,-78.80,35.95,-78.45", aliases: &["raleigh", "durham", "research triangle"] },
    MetroEntry { label: "Memphis metro", location: "Memphis metro, Tennessee, United States", bbox: "35.00,-90.15,35.30,-89.75", aliases: &["memphis"] },
    MetroEntry { label: "Richmond metro", location: "Richmond metro, Virginia, United States", bbox: "37.45,-77.60,37.65,-77.35", aliases: &["richmond"] },
    MetroEntry { label: "New Orleans metro", location: "New Orleans metro, Louisiana, United States", bbox: "29.85,-90.15,30.05,-89.85", aliases: &["new orleans", "nola"] },
    MetroEntry { label: "Louisville metro", location: "Louisville metro, Kentucky, United States", bbox: "38.15,-85.85,38.35,-85.55", aliases: &["louisville"] },
    MetroEntry { label: "Salt Lake City metro", location: "Salt Lake City metro, Utah, United States", bbox: "40.65,-112.05,40.85,-111.80", aliases: &["salt lake city", "slc"] },
    MetroEntry { label: "Hartford metro", location: "Hartford metro, Connecticut, United States", bbox: "41.65,-72.80,41.85,-72.60", aliases: &["hartford"] },
    MetroEntry { label: "Buffalo metro", location: "Buffalo metro, New York, United States", bbox: "42.80,-78.95,42.98,-78.75", aliases: &["buffalo"] },
    MetroEntry { label: "Birmingham metro", location: "Birmingham metro, Alabama, United States", bbox: "33.40,-86.95,33.65,-86.65", aliases: &["birmingham"] },
];

/// How many metros a nationwide search may draw from.
const NATIONWIDE_SAMPLE: usize = 12;

fn normalize(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// True when `needle` appears in `haystack` on whole-word boundaries.
///
/// Substring matching is what made "lawn care" resolve to law firms, so every
/// lookup in this module goes through here.
fn contains_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    format!(" {haystack} ").contains(&format!(" {needle} "))
}

fn nationwide(rotation: usize) -> Vec<BoundedRegion> {
    let pool = METROS.len().min(NATIONWIDE_SAMPLE.max(1));
    (0..pool)
        .map(|offset| {
            let entry = &METROS[(rotation + offset) % METROS.len()];
            BoundedRegion {
                label: entry.label,
                location: entry.location,
                bbox: entry.bbox,
            }
        })
        .collect()
}

/// Wikidata items for each state: (display name, item id, the item's English
/// label). The label lets the discovery lane verify at query time that an id
/// really names the state it is filed under; a mismatch is reported and the
/// state filter skipped, never trusted.
#[rustfmt::skip]
const WIKIDATA_STATES: &[(&str, &str, &str)] = &[
    ("Alabama", "Q173", "Alabama"), ("Alaska", "Q797", "Alaska"), ("Arizona", "Q816", "Arizona"),
    ("Arkansas", "Q1612", "Arkansas"), ("California", "Q99", "California"), ("Colorado", "Q1261", "Colorado"),
    ("Connecticut", "Q779", "Connecticut"), ("Delaware", "Q1393", "Delaware"),
    ("District of Columbia", "Q61", "Washington, D.C."), ("Florida", "Q812", "Florida"),
    ("Georgia", "Q1428", "Georgia"), ("Hawaii", "Q782", "Hawaii"), ("Idaho", "Q1221", "Idaho"),
    ("Illinois", "Q1204", "Illinois"), ("Indiana", "Q1415", "Indiana"), ("Iowa", "Q1546", "Iowa"),
    ("Kansas", "Q1558", "Kansas"), ("Kentucky", "Q1603", "Kentucky"), ("Louisiana", "Q1588", "Louisiana"),
    ("Maine", "Q724", "Maine"), ("Maryland", "Q1391", "Maryland"), ("Massachusetts", "Q771", "Massachusetts"),
    ("Michigan", "Q1166", "Michigan"), ("Minnesota", "Q1527", "Minnesota"), ("Mississippi", "Q1494", "Mississippi"),
    ("Missouri", "Q1581", "Missouri"), ("Montana", "Q1212", "Montana"), ("Nebraska", "Q1553", "Nebraska"),
    ("Nevada", "Q1227", "Nevada"), ("New Hampshire", "Q759", "New Hampshire"), ("New Jersey", "Q1408", "New Jersey"),
    ("New Mexico", "Q1522", "New Mexico"), ("New York", "Q1384", "New York"), ("North Carolina", "Q1454", "North Carolina"),
    ("North Dakota", "Q1207", "North Dakota"), ("Ohio", "Q1397", "Ohio"), ("Oklahoma", "Q1649", "Oklahoma"),
    ("Oregon", "Q824", "Oregon"), ("Pennsylvania", "Q1400", "Pennsylvania"), ("Rhode Island", "Q1387", "Rhode Island"),
    ("South Carolina", "Q1456", "South Carolina"), ("South Dakota", "Q1211", "South Dakota"), ("Tennessee", "Q1509", "Tennessee"),
    ("Texas", "Q1439", "Texas"), ("Utah", "Q829", "Utah"), ("Vermont", "Q16551", "Vermont"),
    ("Virginia", "Q1370", "Virginia"), ("Washington", "Q1223", "Washington"), ("West Virginia", "Q1371", "West Virginia"),
    ("Wisconsin", "Q1537", "Wisconsin"), ("Wyoming", "Q1214", "Wyoming"),
];

/// A state as the Wikidata lane filters by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WikidataState {
    pub name: &'static str,
    pub id: &'static str,
    pub label: &'static str,
}

/// The state a geography names, as a Wikidata filter. `None` for nationwide or
/// unrecognized geographies.
pub fn wikidata_state(geography: &str) -> Option<WikidataState> {
    let state = place(geography)?.state;
    WIKIDATA_STATES
        .iter()
        .find(|(name, _, _)| *name == state)
        .map(|(name, id, label)| WikidataState { name, id, label })
}

/// Where a piece of text places something: a metro when one is named, and
/// always the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub metro: Option<&'static str>,
    pub state: &'static str,
}

/// Locate free text such as "Austin, TX" or "Philadelphia metro, Pennsylvania,
/// United States" to a metro and state, using the same rules as `resolve`.
/// `None` for text that names no state, including "United States" on its own.
pub fn place(text: &str) -> Option<Place> {
    let normalized = normalize(text);
    if normalized.is_empty() {
        return None;
    }
    if let Some(metro) = METROS.iter().find(|entry| {
        contains_word(&normalized, &normalize(entry.label))
            || entry
                .aliases
                .iter()
                .any(|alias| contains_word(&normalized, alias))
    }) {
        let state = STATES.iter().find(|entry| {
            metro
                .location
                .ends_with(&format!(", {}, United States", entry.display))
        })?;
        return Some(Place {
            metro: Some(metro.label),
            state: state.display,
        });
    }
    let last_token = normalized.split(' ').next_back().unwrap_or_default();
    STATES
        .iter()
        .filter(|entry| contains_word(&normalized, entry.name) || last_token == entry.code)
        .max_by_key(|entry| entry.name.len())
        .map(|entry| Place {
            metro: None,
            state: entry.display,
        })
}

/// Resolve a user-supplied geography into bounded sampling regions.
///
/// `rotation` shifts which metros a nationwide or statewide search visits
/// first, so repeating the same search surfaces different companies instead of
/// re-sampling one bounding box forever.
pub fn resolve(geography: &str, rotation: usize) -> Vec<BoundedRegion> {
    let normalized = normalize(geography);
    if normalized.is_empty() {
        return nationwide(rotation);
    }

    if let Some(entry) = METROS.iter().find(|entry| {
        contains_word(&normalized, &normalize(entry.label))
            || entry
                .aliases
                .iter()
                .any(|alias| contains_word(&normalized, alias))
    }) {
        return vec![BoundedRegion {
            label: entry.label,
            location: entry.location,
            bbox: entry.bbox,
        }];
    }

    // A two-letter code is only meaningful as a trailing token ("Austin, TX").
    // Matching it anywhere would make "businesses in Ohio" resolve to Indiana
    // and "sell to or near Boise" resolve to Oregon.
    let last_token = normalized.split(' ').next_back().unwrap_or_default();
    // Longest name wins so "West Virginia" does not resolve to Virginia.
    if let Some(entry) = STATES
        .iter()
        .filter(|entry| contains_word(&normalized, entry.name) || last_token == entry.code)
        .max_by_key(|entry| entry.name.len())
    {
        // A whole state is a large box. Lead with the metros inside it when we
        // know any, so early results come from dense business areas, then fall
        // back to the statewide box for coverage beyond them.
        let suffix = format!(", {}, United States", entry.display);
        let mut regions = METROS
            .iter()
            .filter(|metro| metro.location.ends_with(&suffix))
            .map(|metro| BoundedRegion {
                label: metro.label,
                location: metro.location,
                bbox: metro.bbox,
            })
            .collect::<Vec<_>>();
        let offset = rotation % regions.len().max(1);
        regions.rotate_left(offset);
        regions.push(BoundedRegion {
            label: entry.display,
            location: entry.location,
            bbox: entry.bbox,
        });
        return regions;
    }

    if contains_word(&normalized, "united states")
        || contains_word(&normalized, "usa")
        || contains_word(&normalized, "us")
        || contains_word(&normalized, "u s")
        || contains_word(&normalized, "nationwide")
        || contains_word(&normalized, "america")
    {
        return nationwide(rotation);
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bbox_is_well_formed_and_inside_the_united_states() {
        let boxes = STATES
            .iter()
            .map(|entry| (entry.name, entry.bbox))
            .chain(METROS.iter().map(|entry| (entry.label, entry.bbox)));
        for (name, bbox) in boxes {
            let parts = bbox
                .split(',')
                .map(|part| part.parse::<f64>().expect("bbox part must parse"))
                .collect::<Vec<_>>();
            assert_eq!(parts.len(), 4, "{name} must have four bbox parts");
            let [south, west, north, east] = [parts[0], parts[1], parts[2], parts[3]];
            assert!(south < north, "{name} south must be below north");
            assert!(west < east, "{name} west must be left of east");
            assert!((15.0..=72.0).contains(&south), "{name} south out of range");
            assert!((15.0..=72.0).contains(&north), "{name} north out of range");
            assert!((-180.0..=-64.0).contains(&west), "{name} west out of range");
            assert!((-180.0..=-64.0).contains(&east), "{name} east out of range");
        }
    }

    #[test]
    fn resolves_states_metros_and_nationwide() {
        assert_eq!(resolve("Ohio", 0).last().unwrap().label, "Ohio");
        assert_eq!(resolve("Seattle", 0)[0].label, "Seattle metro");
        assert_eq!(resolve("Austin, TX", 0)[0].label, "Austin metro");
        assert_eq!(resolve("TX", 0).last().unwrap().label, "Texas");
        assert_eq!(resolve("United States", 0).len(), NATIONWIDE_SAMPLE);
        assert!(resolve("Everywhere on earth", 0).is_empty());
    }

    #[test]
    fn places_free_text_at_metro_and_state() {
        assert_eq!(
            place("Austin, TX"),
            Some(Place {
                metro: Some("Austin metro"),
                state: "Texas"
            })
        );
        assert_eq!(
            place("Philadelphia metro, Pennsylvania, United States").map(|p| p.state),
            Some("Pennsylvania")
        );
        assert_eq!(
            place("Haddonfield, New Jersey, United States"),
            Some(Place {
                metro: None,
                state: "New Jersey"
            })
        );
        assert_eq!(
            place("West Virginia").map(|p| p.state),
            Some("West Virginia")
        );
        assert_eq!(place("United States"), None);
    }

    #[test]
    fn every_state_has_a_wikidata_filter() {
        for entry in STATES {
            let scoped = wikidata_state(entry.display)
                .unwrap_or_else(|| panic!("{} has no Wikidata id", entry.display));
            assert_eq!(scoped.name, entry.display);
            assert!(scoped.id.starts_with('Q'));
        }
        assert_eq!(wikidata_state("Austin, TX").map(|s| s.id), Some("Q1439"));
        assert_eq!(wikidata_state("United States"), None);
        assert!(resolve("Paris, France", 0).is_empty());
    }

    #[test]
    fn state_codes_only_match_as_a_trailing_token() {
        // "in" is Indiana's code and "or" is Oregon's. Matching them anywhere
        // in the phrase silently retargeted the whole search.
        assert_eq!(
            resolve("businesses in Ohio", 0).last().unwrap().label,
            "Ohio"
        );
        assert_eq!(
            resolve("plumbers or roofers in Texas", 0)
                .last()
                .unwrap()
                .label,
            "Texas"
        );
        assert!(resolve("companies in", 0).last().unwrap().label == "Indiana");
    }

    #[test]
    fn longest_state_name_wins() {
        // "West Virginia" contains "Virginia" as a whole word.
        assert_eq!(
            resolve("West Virginia", 0).last().unwrap().label,
            "West Virginia"
        );
        assert_eq!(resolve("Virginia", 0).last().unwrap().label, "Virginia");
        // Word boundaries already keep "Arkansas" away from "Kansas".
        assert_eq!(resolve("Arkansas", 0).last().unwrap().label, "Arkansas");
    }

    #[test]
    fn previously_unsupported_geographies_now_resolve() {
        // Before the region table these all returned an empty vec, which the
        // caller turned into "Automatic map discovery is unavailable".
        for geography in [
            "Ohio",
            "New Jersey",
            "Washington",
            "Seattle",
            "Austin",
            "Raleigh, NC",
            "Salt Lake City",
        ] {
            assert!(
                !resolve(geography, 0).is_empty(),
                "{geography} should resolve to at least one bounded region"
            );
        }
    }

    #[test]
    fn word_boundary_matching_avoids_substring_collisions() {
        // "us" must not fire inside unrelated words, and a foreign city that
        // merely contains a state code must not resolve.
        assert!(resolve("Toulouse", 0).is_empty());
        assert_eq!(resolve("Indiana", 0).last().unwrap().label, "Indiana");
    }

    #[test]
    fn rotation_changes_which_regions_are_sampled_first() {
        let first = resolve("United States", 0);
        let second = resolve("United States", 5);
        assert_ne!(first[0].label, second[0].label);
        assert_eq!(first.len(), second.len());
    }

    #[test]
    fn state_searches_lead_with_metros_then_cover_the_whole_state() {
        let ohio = resolve("Ohio", 0);
        assert!(ohio.len() > 1, "Ohio should include its metros");
        assert!(ohio
            .iter()
            .any(|region| region.label == "Cleveland metro" || region.label == "Columbus metro"));
        assert_eq!(ohio.last().unwrap().bbox, "38.403,-84.820,41.978,-80.519");
    }

    #[test]
    fn bounded_locations_name_a_us_state() {
        for entry in METROS {
            assert!(
                entry.location.ends_with(", United States"),
                "{} must carry a full U.S. location string",
                entry.label
            );
        }
    }
}
