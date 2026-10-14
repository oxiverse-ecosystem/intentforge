//! Generic affiliate decoration engine (ROADMAP item 3).
//!
//! Server-side, data-driven affiliate decoration. NO hardcoded merchant/network
//! knowledge lives in code — everything (network list, template, params, key env
//! var name, rotation ids) comes from `data/commerce/*.json`, so a new
//! merchant/network is added by editing DATA ONLY. The engine is a generic
//! template renderer supporting two kinds:
//!   * `wrap`        — network prefix + url-encoded destination (Sovrn/Skimlinks)
//!   * `append_params` — add query params to the merchant URL (EPN/Amazon/most)
//!
//! Decoration is a STRICT post-ranking pass: `decorate_affiliate` iterates the
//! ALREADY-RANKED results and mutates only each result's `affiliate` field.
//! It never reorders, reselects, or re-scores. Missing key => `affiliate: null`,
//! result still returned (graceful degradation, never a crash / never breaks
//! search). No user id / session id / IP / query text is ever placed in an
//! affiliate parameter (privacy rule) — the only coarse subid is the merchant
//! host, which is non-identifying.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Affiliate template engine (ROADMAP item 3) ──────────────────────
// Server-side, data-driven affiliate decoration. NO hardcoded merchant/network
// knowledge lives in code — everything (network list, template, params, key env
// var name, rotation ids) comes from `data/commerce/affiliate.json`, so a new
// merchant/network is added by editing DATA ONLY. The engine is a generic
// template renderer supporting two kinds:
//   * `wrap`        — network prefix + url-encoded destination (Sovrn/Skimlinks)
//   * `append_params` — add query params to the merchant URL (EPN/Amazon/most)
// Decoration is a STRICT post-ranking pass: `decorate_affiliate` iterates the
// ALREADY-RANKED results and mutates only each result's `affiliate` field.
// It never reorders, reselects, or re-scores. Missing key => `affiliate: null`,
// result still returned (graceful degradation, never a crash / never breaks
// search). No user id / session id / IP / query text is ever placed in an
// affiliate parameter (privacy rule) — the only coarse subid is the merchant
// host, which is non-identifying.
/// One network/program row loaded from the data file. Generic over kinds.
///
/// `host_suffix` enables per-merchant matching: when present, this program
/// only applies to result URLs whose host equals or ends with the suffix
/// (label-boundary match, case-insensitive). When null, the program is a
/// catch-all (matches any URL) — preserving the original single-network
/// behaviour for backward compatibility.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct AffiliateNetwork {
    id: String,
    #[serde(default)]
    kind: String, // "wrap" | "append_params"
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_priority")]
    priority: i64,
    #[serde(default)]
    network: String,
    template: String,
    #[serde(default)]
    params: std::collections::HashMap<String, String>,
    #[serde(default)]
    key_env: Option<String>,
    #[serde(default)]
    param_env: HashMap<String, String>,
    /// Bid floor (Sovrn `bf`) appended as a query param on the decorated URL.
    /// DATA-ONLY — never influences ranking (decoration is a strict post-ranking
    /// pass). Surfaced in the `affiliate` block for reporting.
    #[serde(default)]
    bid_floor: Option<String>,
    /// Fallback URL (Sovrn `fbu`) used if the bid floor isn't met. Appended
    /// url-encoded as a query param. DATA-ONLY.
    #[serde(default)]
    fallback_url: Option<String>,
    /// Optional merchant host suffix for per-merchant matching (e.g.
    /// "amazon." matches amazon.com, amazon.co.uk, etc.). Null = catch-all.
    #[serde(default)]
    host_suffix: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_priority() -> i64 {
    0
}

/// Network metadata loaded from `data/commerce/networks.json`. Provides
/// disclosure labels and subid policy documentation for each network.
/// Purely informational — does not affect link rendering.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct AffiliateNetworkMeta {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    disclosure_label: String,
    #[serde(default)]
    subid_policy: String,
    #[serde(default)]
    subid_description: String,
}

/// HOST component of a URL, lowercased, without scheme/port/userinfo/path.
/// Pure; falls back to the raw string when the value has no `://` scheme.
fn url_host(value: &str) -> String {
    let after_scheme = match value.find("://") {
        Some(i) => &value[i + 3..],
        None => value,
    };
    let authority_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];
    // Strip userinfo (`user:pass@host`) then any `:port`.
    let host = match authority.rfind('@') {
        Some(i) => &authority[i + 1..],
        None => authority,
    };
    let host = match host.rfind(':') {
        Some(i) if host[i + 1..].chars().all(|c| c.is_ascii_digit()) => &host[..i],
        _ => host,
    };
    host.trim_end_matches('.').to_lowercase()
}

/// TRUE when `host` carries a label reserved for documentation by IANA and
/// therefore can never be a real merchant destination.
///
/// Encodes two published registries — no merchant, network, or query literals:
///   * RFC 2606 §2 / RFC 6761 §6 — reserved NAMES: `example`, `test`,
///     `invalid`, `localhost`, and their `.tld`/`.test`/`.example`/`.invalid`
///     forms;
///   * RFC 2606 §2 — the `example` documentation PREFIX: a label beginning with
///     `example` followed by a separator (`example-merchant`, `example_shop`).
///     This is what catches the common "invented merchant" shape, which is
///     legal DNS but is a placeholder by convention.
///
/// Matching is LABEL-BOUNDARY, never substring, and the prefix rule applies ONLY
/// to `example`:
///   * `test` is a special-use NAME, but `testosterone-shop.com` is a real shop,
///     so a label merely CONTAINING a reserved name must be accepted;
///   * `invalid` likewise (RFC 2606 reserves the exact name, not the prefix), so
///     `invalid-syntax.co.uk` must not be rejected.
///
/// Used to drop an `fbu` (bid-miss fallback) that would send a real user to a
/// fabricated destination. The FIELD is dropped, never the network: wrapping,
/// `bf` and `disclosed: true` all keep working.
fn is_reserved_documentation_host(host: &str) -> bool {
    let h = host.trim().trim_end_matches('.').to_lowercase();
    if h.is_empty() {
        return false;
    }
    const RESERVED_NAMES: [&str; 4] = ["example", "test", "invalid", "localhost"];
    for label in h.split('.') {
        if RESERVED_NAMES.contains(&label) {
            return true;
        }
        // `example` is additionally a documentation PREFIX (RFC 2606 §2):
        // `example-merchant` / `example_shop` are placeholders by convention.
        if let Some(rest) = label.strip_prefix("example") {
            if rest
                .chars()
                .next()
                .map(|c| !c.is_ascii_alphanumeric())
                .unwrap_or(false)
            {
                return true;
            }
        }
    }
    false
}

/// Runtime-resolved config: data files + env-resolved keys. Built once at startup.
///
/// `networks` holds the legacy catch-all programs (from `affiliate.json`).
/// `programs` holds per-merchant programs (from `affiliate_programs.json`) —
/// each carries a `host_suffix` for domain matching. `network_meta` holds
/// disclosure/subid metadata (from `networks.json`).
///
/// Decoration logic: for each result, the engine first tries to match a
/// per-merchant program by `host_suffix`. If none matches, it falls back to
/// the first usable catch-all network. This preserves backward compatibility
/// with the single-network model while enabling per-merchant decoration.
#[derive(Clone)]
struct AffiliateCtx {
    networks: Vec<AffiliateNetwork>,
    programs: Vec<AffiliateNetwork>,
    network_meta: Vec<AffiliateNetworkMeta>,
}

/// Host labels that are IANA-reserved and can never be a real merchant:
/// matched EXACTLY as a host label. RFC 2606 §2 reserves `example`; RFC 6761
/// reserves the `test` / `invalid` / `localhost` special-use names.
///
/// Exact matching only — deliberately. `invalid` is a special-use NAME, not a
/// documentation PREFIX, so prefix-matching it would wrongly reject real hosts
/// like `invalid-syntax.co.uk`.
const RESERVED_HOST_LABELS: &[&str] = &["example", "test", "invalid", "localhost"];

/// Host-label PREFIXES reserved by the documentation-label convention. A label
/// beginning with one of these followed by a separator (`example-merchant`,
/// `example-shop`) is a documentation placeholder, not a merchant.
///
/// This is a separate list from `RESERVED_HOST_LABELS` precisely because
/// prefix-matching is only correct for `example`: `example-merchant.com` is its
/// OWN registrable domain, NOT a subdomain of `example.com`, so a pure
/// suffix/exact check on the reserved set misses the exact value that shipped in
/// production config.
const RESERVED_HOST_LABEL_PREFIXES: &[&str] = &["example"];

/// True when any label of `url`'s host marks it as a documentation/example
/// placeholder, i.e. it cannot be a real merchant destination.
///
/// Rejected shapes:
///   * a label exactly equal to a reserved name (`example.com`, `foo.test`), and
///   * a label beginning with a reserved documentation prefix + separator
///     (`example-merchant.com`, `example-shop.co.uk`).
///
/// Matching is always on a LABEL boundary: a real host that merely CONTAINS a
/// reserved word as a substring (`notexample.com`, `testosterone-shop.com`,
/// `invalid-syntax.co.uk`) is NOT rejected, so the guard cannot silently disable
/// a legitimate merchant fallback.
///
/// Returns `true` for unparseable/host-less input: a fallback we cannot even
/// resolve is treated as unusable rather than shipped.
fn is_reserved_placeholder_url(url: &str) -> bool {
    let host = match reqwest::Url::parse(url) {
        Ok(u) => match u.host_str() {
            Some(h) => h.trim_end_matches('.').to_ascii_lowercase(),
            None => return true,
        },
        Err(_) => return true,
    };
    let separated_prefix = |label: &str, prefix: &str| {
        label
            .strip_prefix(prefix)
            .map(|rest| rest.starts_with('-') || rest.starts_with('_'))
            .unwrap_or(false)
    };
    host.split('.').any(|label| {
        RESERVED_HOST_LABELS.iter().any(|r| label == *r)
            || RESERVED_HOST_LABEL_PREFIXES
                .iter()
                .any(|p| separated_prefix(label, p))
    })
}

impl AffiliateCtx {
    /// Load networks from the data files. An empty/missing file is NOT fatal: the
    /// engine simply has no networks and every result degrades to `affiliate:
    /// null`. This is intentional — affiliate decoration is best-effort and must
    /// never break search.
    ///
    /// Loads three data files:
    ///   1. `affiliate.json`         — legacy catch-all networks (backward compat)
    ///   2. `affiliate_programs.json` — per-merchant programs with host_suffix matching
    ///   3. `networks.json`          — network metadata (disclosure labels, subid policy)
    fn load() -> Self {
        let mut networks: Vec<AffiliateNetwork> = Vec::new();
        let mut programs: Vec<AffiliateNetwork> = Vec::new();
        let mut network_meta: Vec<AffiliateNetworkMeta> = Vec::new();

        // Resolve the data path relative to the process cwd (container WORKDIR is
        // /app, app binary at /app/gateway; data baked at /app/data/commerce).
        let candidates = [
            "data/commerce",
            "/app/data/commerce",
            "./data/commerce",
        ];

        // ── 1. Legacy catch-all networks (affiliate.json) ──
        for base in &candidates {
            let path = format!("{}/affiliate.json", base);
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(arr) = v.get("networks").and_then(|n| n.as_array()) {
                        for n in arr {
                            if let Ok(mut net) = serde_json::from_value::<AffiliateNetwork>(n.clone()) {
                                // Never accept a non-routable documentation domain as
                                // a real merchant fallback. Dropping the FIELD (not the
                                // whole network) keeps the network usable for wrapping
                                // while guaranteeing no click is ever routed to a
                                // fabricated destination.
                                if let Some(fbu) = net.fallback_url.clone() {
                                    if is_reserved_placeholder_url(&fbu) {
                                        tracing::warn!(
                                            network = %net.id,
                                            "affiliate: ignoring reserved placeholder fallback_url (RFC 2606/6761 documentation domain)"
                                        );
                                        net.fallback_url = None;
                                    }
                                }
                                networks.push(net);
                            }
                        }
                    }
                }
                if !networks.is_empty() {
                    break;
                }
            }
        }

        // ── 2. Per-merchant programs (affiliate_programs.json) ──
        for base in &candidates {
            let path = format!("{}/affiliate_programs.json", base);
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(arr) = v.get("programs").and_then(|n| n.as_array()) {
                        for n in arr {
                            if let Ok(mut net) = serde_json::from_value::<AffiliateNetwork>(n.clone()) {
                                // Same fallback sanitization as legacy networks.
                                if let Some(fbu) = net.fallback_url.clone() {
                                    if is_reserved_placeholder_url(&fbu) {
                                        tracing::warn!(
                                            program = %net.id,
                                            "affiliate: ignoring reserved placeholder fallback_url (RFC 2606/6761 documentation domain)"
                                        );
                                        net.fallback_url = None;
                                    }
                                }
                                programs.push(net);
                            }
                        }
                    }
                }
                if !programs.is_empty() {
                    break;
                }
            }
        }

        // ── 3. Network metadata (networks.json) ──
        for base in &candidates {
            let path = format!("{}/networks.json", base);
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(arr) = v.get("networks").and_then(|n| n.as_array()) {
                        for n in arr {
                            if let Ok(meta) = serde_json::from_value::<AffiliateNetworkMeta>(n.clone()) {
                                network_meta.push(meta);
                            }
                        }
                    }
                }
                if !network_meta.is_empty() {
                    break;
                }
            }
        }

        networks.sort_by(|a, b| b.priority.cmp(&a.priority));
        programs.sort_by(|a, b| b.priority.cmp(&a.priority));
        tracing::info!(
            "affiliate: loaded {} network(s), {} program(s), {} network meta(s) from data files",
            networks.len(),
            programs.len(),
            network_meta.len()
        );
        Self { networks, programs, network_meta }
    }

    /// Construct an AffiliateCtx with only legacy catch-all networks (no
    /// per-merchant programs, no network metadata). Used by tests that build
    /// synthetic contexts.
    fn from_networks(networks: Vec<AffiliateNetwork>) -> Self {
        Self { networks, programs: vec![], network_meta: vec![] }
    }

    /// The first enabled network that has its required key present in the env.
    /// Returns None when no network can be applied (=> `affiliate: null`).
    fn first_usable(&self) -> Option<&AffiliateNetwork> {
        self.networks.iter().find(|n| {
            n.enabled
                && n.key_env
                    .as_ref()
                    .map(|k| std::env::var(k).is_ok())
                    .unwrap_or(false)
        })
    }

    /// Find the first enabled per-merchant program whose `host_suffix` matches
    /// the given URL's host AND whose key is present in the env.
    ///
    /// `host_suffix` matching is label-boundary, case-insensitive: the result
    /// URL's host must equal the suffix or end with `"." + suffix` (e.g.
    /// suffix `"amazon."` matches `amazon.com`, `amazon.co.uk`, etc.).
    ///
    /// Returns None when no per-merchant program matches (caller should fall
    /// back to `first_usable()` for the legacy catch-all path).
    fn match_program(&self, url: &str) -> Option<&AffiliateNetwork> {
        let host = url_host(url);
        if host.is_empty() {
            return None;
        }
        // Collect all matching programs, then pick the highest priority.
        // This ensures deterministic behavior regardless of vector order.
        let mut best: Option<&AffiliateNetwork> = None;
        for p in &self.programs {
            if !p.enabled {
                continue;
            }
            let has_key = p.key_env.as_ref().map(|k| std::env::var(k).is_ok()).unwrap_or(false);
            if !has_key {
                continue;
            }
            let matches = match &p.host_suffix {
                None => false,
                Some(suffix) => {
                    let suffix = suffix.trim_end_matches('.').to_lowercase();
                    if suffix.is_empty() {
                        false
                    } else if host == suffix {
                        true
                    } else {
                        host.split('.').any(|label| label == suffix)
                    }
                }
            };
            if !matches {
                continue;
            }
            best = match best {
                None => Some(p),
                Some(current) if p.priority > current.priority => Some(p),
                _ => best,
            };
        }
        best
    }
}

/// Build the affiliate URL for a (network, destination) without touching any
/// result. Pure + unit-testable. `subid` is the coarse, non-identifying value
/// (merchant host) — see privacy rule above.
///
/// Both template kinds share ONE generic renderer:
///   * `wrap`          — the template is a NETWORK PREFIX; the destination URL
///                       must be percent-encoded (it is a query-param value), and
///                       the network's `params` are appended as extra query pairs
///                       (e.g. Sovrn `cuid={subid}`).
///   * `append_params` — the template IS the raw destination URL; `params` are
///                       appended (encoded) directly to it (e.g. eBay EPN / Amazon).
/// Resolved value sources (all data, never code):
///   {key}            env var named by `key_env`
///   {url}            the (encoded for wrap / raw for append_params) destination
///   {subid}          the coarse merchant host
///   {param:<name>}   env var named by `param_env[name]` (generic — any name)
fn render_affiliate_url(net: &AffiliateNetwork, dest_url: &str, subid: &str) -> String {
    let key = net
        .key_env
        .as_ref()
        .and_then(|k| std::env::var(k).ok())
        .unwrap_or_default();

    // Resolve {param:<name>} placeholders from the network's param_env.
    let mut resolved_params: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (pname, env_key) in &net.param_env {
        if let Ok(v) = std::env::var(env_key) {
            resolved_params.insert(pname.clone(), v);
        }
    }

    // For `wrap` the destination must be url-encoded (it is a query-param value
    // inside the network's prefix); for `append_params` the template IS the raw
    // destination URL, so the `{url}` token must stay un-encoded.
    let url_token = if net.kind == "wrap" {
        urlencoding::encode(dest_url).into_owned()
    } else {
        dest_url.to_string()
    };
    let fill = |s: &str| -> String {
        // Generic {param:<name>} resolution — any name, not just campaign_id.
        let mut out = s.to_string();
        // First interpolate scalar tokens.
        out = out.replace("{key}", &key);
        out = out.replace("{url}", &url_token);
        out = out.replace("{subid}", subid);
        // Then replace every {param:<name>} we have resolved from env.
        for (pname, pval) in &resolved_params {
            out = out.replace(&format!("{{param:{}}}", pname), pval);
        }
        // Any un-resolved {param:*} (missing env) collapses to empty — never a
        // fabricated value, never a leftover literal placeholder.
        let leftover = regex::Regex::new(r"\{param:[^}]*\}")
            .ok()
            .and_then(|re| re.find(&out).map(|_| true))
            .unwrap_or(false);
        if leftover {
            // Strip the bare {param:...} token entirely.
            if let Ok(re) = regex::Regex::new(r"\{param:[^}]*\}") {
                out = re.replace_all(&out, "").to_string();
            }
        }
        out
    };

    // The base template, with scalar + {param:*} tokens filled.
    let base = fill(&net.template);

    // Merge the network's own `params` plus the generic bid-floor / fallback
    // fields (ROADMAP item 6) into one query-pair list. ALL THREE are
    // DATA-DRIVEN — none is hardcoded here. `bf`/`fbu` are pure reporting/fallback
    // signals that NEVER affect ranking: decoration runs strictly AFTER the ranked
    // order is fixed (see `decorate_affiliate` + the order-invariance test), so
    // nothing appended here can move a result's position.
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (k, v) in &net.params {
        let val = fill(v);
        if val.is_empty() {
            continue;
        }
        pairs.push((k.clone(), val));
    }
    if let Some(bf) = &net.bid_floor {
        let val = fill(bf);
        if !val.is_empty() {
            pairs.push(("bf".to_string(), val));
        }
    }
    if let Some(fbu) = &net.fallback_url {
        // fbu is itself a URL. We must NOT pre-encode it here: the final
        // query-pair encoding step below encodes every value exactly once, so
        // passing the raw URL avoids a double-encoding bug (raw => encoded once).
        let val = fbu.clone();
        if !val.is_empty() {
            pairs.push(("fbu".to_string(), val));
        }
    }
    if !pairs.is_empty() {
        let encoded: Vec<String> = pairs
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect();
        if base.contains('?') {
            return format!("{}&{}", base, encoded.join("&"));
        } else {
            return format!("{}?{}", base, encoded.join("&"));
        }
    }
    base
}

/// STRICT POST-RANKING decoration pass. Takes the ALREADY-RANKED `results` and
/// adds an `affiliate` block to each, preserving order byte-for-byte. A result
/// that already carries an `affiliate` block is left untouched (idempotent).
/// When no usable network key is present, results keep `affiliate: null` and are
/// still returned. This function NEVER reorders or re-scores.
///
/// Per-merchant matching: for each result, the engine first tries to find a
/// per-merchant program (from `affiliate_programs.json`) whose `host_suffix`
/// matches the result URL's host. If found, that program's template/params are
/// used. If no per-merchant program matches, the engine falls back to the
/// first usable catch-all network (from `affiliate.json`) — preserving the
/// original single-network behaviour. This two-tier lookup is fully data-driven:
/// adding a new merchant requires only editing `affiliate_programs.json`.
fn decorate_affiliate(results: &mut [serde_json::Value], ctx: &AffiliateCtx) {
    for r in results.iter_mut() {
        if !r.is_object() {
            continue;
        }
        if r.get("affiliate").is_some() {
            continue; // idempotent: never double-wrap
        }
        let url = match r.get("url").and_then(|v| v.as_str()) {
            Some(u) if !u.is_empty() => u.to_string(),
            _ => continue,
        };

        // Two-tier lookup: per-merchant program first, then catch-all fallback.
        let net = ctx.match_program(&url).or_else(|| ctx.first_usable());
        let net = match net {
            Some(n) => n,
            None => continue, // graceful: no key => leave affiliate un-set
        };

        // Coarse, non-identifying subid: the destination merchant host only.
        // PRIVACY: never add user id, session id, IP, or query text to subid.
        // The subid is ONLY the merchant host (e.g. "amazon.com"), which is
        // coarse and non-identifying — it tells the network which merchant the
        // click is for, nothing about the user.
        let subid = reqwest::Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .unwrap_or_default();
        let aff_url = render_affiliate_url(net, &url, &subid);
        // Reporting fields (ROADMAP item 6): surface the configured bid floor /
        // fallback so operators can report on monetization. These are DATA-driven
        // (from the network config) and are NEVER used for ranking. `disclosed`
        // stays true so the frontend can label the result.
        let mut block = serde_json::Map::new();
        block.insert(
            "network".to_string(),
            serde_json::Value::String(net.network.clone()),
        );
        block.insert("url".to_string(), serde_json::Value::String(aff_url));
        block.insert("disclosed".to_string(), serde_json::Value::Bool(true));
        if let Some(bf) = &net.bid_floor {
            block.insert("bid_floor".to_string(), serde_json::Value::String(bf.clone()));
        }
        if let Some(fbu) = &net.fallback_url {
            block.insert("fallback".to_string(), serde_json::Value::String(fbu.clone()));
        }
        r["affiliate"] = serde_json::Value::Object(block);
    }
}

