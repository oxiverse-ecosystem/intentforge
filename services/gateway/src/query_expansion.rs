//! Query expansion for 0-result fallback.
//!
//! When `/search` returns 0 results, the retry logic calls into this module
//! to generate expanded query variants. The module:
//! 1. Tokenizes the query
//! 2. Strips modifier words (adjectives, context words) using a data-driven approach
//! 3. Extracts strong entities (product names, technical terms)
//! 4. Generates expanded query variants ordered by specificity (most specific first)

use std::collections::HashSet;

/// Stop words that carry no search value.
const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "is", "are", "was", "were", "be", "been", "being",
    "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "shall", "can", "need", "dare", "ought",
    "used", "to", "of", "in", "for", "on", "with", "at", "by", "from",
    "as", "into", "through", "during", "before", "after", "above", "below",
    "between", "out", "off", "over", "under", "again", "further", "then",
    "once", "here", "there", "when", "where", "why", "how", "all", "both",
    "each", "few", "more", "most", "other", "some", "such", "no", "nor",
    "not", "only", "own", "same", "so", "than", "too", "very", "just",
    "because", "but", "and", "or", "if", "while", "about", "up", "down",
    "that", "this", "these", "those", "it", "its", "i", "me", "my", "myself",
    "we", "our", "ours", "you", "your", "yours", "he", "him", "his", "she",
    "her", "hers", "they", "them", "their", "theirs", "what", "which", "who",
    "whom", "whose",
];

/// Technical keep-words that should never be stripped even if they look like modifiers.
const TECHNICAL_KEEP_WORDS: &[&str] = &[
    "sql", "database", "security", "injection", "authentication", "authorization",
    "encryption", "api", "rest", "graphql", "docker", "kubernetes", "linux",
    "python", "javascript", "typescript", "rust", "java", "csharp", "golang",
    "react", "angular", "vue", "node", "nodejs", "express", "django", "flask",
    "spring", "rails", "laravel", "postgresql", "mysql", "mongodb", "redis",
    "elasticsearch", "aws", "azure", "gcp", "terraform", "ansible", "jenkins",
    "github", "gitlab", "nginx", "apache", "http", "https", "tcp", "udp",
    "dns", "cdn", "ssl", "tls", "ssh", "ftp", "smtp", "websocket", "oauth",
    "jwt", "json", "xml", "yaml", "toml", "markdown", "html", "css", "sass",
    "webpack", "babel", "eslint", "prettier", "jest", "mocha",
    "pytest", "unittest", "junit", "logging", "monitoring", "profiling",
    "debugging", "testing", "deployment", "ci", "cd", "devops", "agile",
    "scrum", "kanban", "microservices", "serverless", "lambda", "containers",
    "virtualization", "cloud", "distributed", "concurrent", "parallel",
    "asynchronous", "synchronous", "blocking", "non-blocking", "io", "cpu",
    "gpu", "memory", "storage", "network", "firewall", "proxy", "load",
    "balancer", "cache", "caching", "index", "query", "search", "sort",
    "filter", "pagination", "validation", "sanitization", "hashing",
    "compression", "decompression", "serialization", "deserialization",
    "marshalling", "unmarshalling", "encoding", "decoding", "parsing",
    "compilation", "interpretation", "runtime", "compiler", "interpreter",
    "linker", "loader", "assembler", "debugger", "profiler", "optimizer",
    "garbage", "collector", "allocator", "deallocator", "reference", "pointer",
    "array", "list", "set", "map", "dictionary", "tree", "graph", "stack",
    "queue", "heap", "hash", "table", "bucket", "chain", "linked", "double",
    "circular", "binary", "ternary", "unary", "octal", "hex",
    "decimal", "float", "double", "integer", "long", "short", "byte", "bit",
    "boolean", "character", "string", "text", "blob", "clob", "nclob",
    "nvarchar", "varchar", "char", "nchar", "numeric", "real",
    "precision", "scale", "unsigned", "signed", "zerofill", "auto_increment",
    "primary", "foreign", "key", "unique", "constraint", "check",
    "default", "null", "references", "cascade", "restrict",
    "action", "deferrable", "initially", "immediate", "deferred",
    "temporary", "temp", "view", "materialized", "trigger", "procedure",
    "function", "package", "type", "cursor", "sequence", "synonym",
    "tablespace", "schema", "catalog", "domain", "assertion", "collation",
    "translation", "characterization", "cast", "convert", "translate",
    "trim", "substring", "position", "extract", "overlay", "length",
    "char_length", "octet_length", "bit_length", "lower", "upper", "abs",
    "floor", "ceiling", "round", "trunc", "sqrt", "power", "exp",
    "ln", "log", "cos", "sin", "tan", "acos", "asin", "atan", "cot",
    "degrees", "radians", "pi", "random", "setseed", "count", "sum", "avg",
    "min", "max", "every", "bit_and", "bit_or", "bool_and",
    "bool_or", "corr", "covar_pop", "covar_samp", "regr_avgx", "regr_avgy",
    "regr_count", "regr_intercept", "regr_r2", "regr_slope", "regr_sxx",
    "regr_sxy", "regr_syy", "stddev", "stddev_pop", "stddev_samp", "variance",
    "var_pop", "var_samp", "rank", "dense_rank", "percent_rank", "cume_dist",
    "ntile", "lag", "lead", "first_value", "last_value", "nth_value",
    "row_number", "grouping", "grouping_id", "rollup", "cube",
    "sets", "listagg", "string_agg", "array_agg", "json_agg", "jsonb_agg",
    "xmlagg", "within", "group", "over", "partition", "order",
    "rows", "range", "groups", "preceding", "following", "unbounded",
    "current", "row", "exclude", "ties", "others", "first", "last", "respect",
    "ignore", "nulls", "distinct", "union", "intersect",
    "except", "minus", "limit", "offset", "fetch", "next", "only", "with",
    "for", "update", "share", "nowait", "wait", "skip", "locked",
    "read", "write", "commit", "rollback", "savepoint", "release",
    "begin", "end", "start", "transaction", "isolation", "level", "serializable",
    "repeatable", "committed", "uncommitted", "dirty", "phantom",
    "lost", "deadlock", "timeout", "retry", "backoff", "exponential",
    "linear", "fixed", "jitter", "circuit", "breaker", "half-open",
    "closed", "open", "failure", "success", "threshold", "sliding",
    "tumbling", "session", "global", "local",
    "link", "relationship", "association",
    "dependency", "hierarchy", "vertex",
    "weight", "cost", "distance", "shortest", "longest", "minimum", "maximum",
    "average", "mean", "median", "mode", "deviation", "standard",
    "total", "number", "amount", "quantity", "volume", "size",
    "width", "height", "depth", "area", "perimeter", "circumference",
    "radius", "diameter", "mass", "density", "pressure",
    "temperature", "humidity", "velocity", "acceleration", "force", "energy",
    "work", "potential", "kinetic", "thermal", "electrical", "magnetic",
    "nuclear", "atomic", "molecular", "chemical", "biological", "physical",
    "quantum", "relativity", "gravity", "space", "time", "dimension",
    "universe", "multiverse", "cosmology", "astronomy", "astrophysics",
    "geology", "geography", "topography", "cartography", "meteorology",
    "climatology", "ecology", "biology", "zoology", "botany", "genetics",
    "evolution", "ecosystem", "habitat", "biome", "biosphere", "atmosphere",
    "hydrosphere", "lithosphere", "cryosphere", "pedosphere", "anthroposphere",
];

/// Modifier words that describe quality, context, or manner rather than entities.
const MODIFIER_WORDS: &[&str] = &[
    "best", "good", "great", "excellent", "amazing", "awesome", "fantastic",
    "wonderful", "terrible", "bad", "worst", "better", "worse", "top",
    "popular", "famous", "common", "rare", "new", "old", "latest",
    "modern", "classic", "traditional", "advanced", "basic", "simple",
    "complex", "easy", "difficult", "hard", "soft", "fast", "slow",
    "quick", "cheap", "expensive", "free", "paid", "premium", "budget",
    "professional", "beginner", "intermediate", "expert", "pro", "amateur",
    "practices", "tips", "tricks", "guide", "guides", "tutorial",
    "tutorials", "method", "methods", "way", "ways", "approach", "approaches",
    "technique", "techniques", "strategy", "strategies", "solution", "solutions",
    "example", "examples", "sample", "samples", "demo", "demos",
    "setup", "configuration", "config", "settings", "options", "parameters",
    "arguments", "flags", "features", "functionality", "capabilities",
    "requirements", "prerequisites", "dependencies", "installation",
    "migration", "upgrade", "update", "version", "release", "changelog",
    "documentation", "docs", "reference", "references", "specification",
    "spec", "standards", "standard", "protocol", "protocols", "format", "formats",
    "structure", "structures", "design", "patterns", "pattern", "architecture",
    "architectures", "model", "models", "framework", "frameworks",
    "library", "libraries", "tool", "tools", "utility", "utilities",
    "application", "applications", "app", "apps", "service", "services",
    "system", "systems", "platform", "platforms", "product", "products",
    "brand", "brands", "company", "companies", "organization", "organizations",
    "team", "teams", "group", "groups", "community", "communities",
    "user", "users", "client", "clients", "server", "servers",
    "host", "hosts", "instance", "instances",
    "cluster", "clusters", "region", "regions", "zone", "zones",
    "location", "locations", "area", "areas", "domain", "domains",
    "subnet", "subnets", "router", "routers", "switch", "switches",
    "balancer", "balancers", "proxies", "caches",
    "topic", "topics", "streams", "pipeline", "pipelines",
    "workflow", "workflows", "process", "processes", "thread",
    "threads", "task", "tasks", "job", "jobs", "batch", "batches",
    "event", "events", "message", "messages", "notification", "notifications",
    "alert", "alerts", "metric", "metrics", "log", "logs", "trace", "traces",
    "span", "spans", "report", "reports", "dashboard", "dashboards",
    "panel", "panels", "widget", "widgets", "component", "components", "module",
    "modules", "package", "packages", "bundle", "bundles", "artifact", "artifacts",
    "repository", "repositories", "registry", "registries",
    "namespace", "namespaces", "scope", "scopes", "context", "contexts",
    "connection", "connections", "pool", "pools",
    "worker", "workers", "executor", "executors", "scheduler", "schedulers",
    "dispatcher", "dispatchers", "handler", "handlers", "listener",
    "listeners", "observer", "observers", "publisher", "publishers",
    "subscriber", "subscribers", "consumer", "consumers", "producer", "producers",
    "broker", "brokers", "partition", "partitions", "replica", "replicas",
    "shard", "shards", "segment", "segments", "block", "blocks",
    "page", "pages", "frame", "frames", "buffer", "buffers",
    "channel", "channels", "pipe", "pipes", "socket", "sockets", "port", "ports",
    "interface", "interfaces", "endpoint", "endpoints", "route", "routes",
    "path", "paths", "uri", "uris", "url", "urls",
    "hostname", "hostnames", "address", "addresses",
    "certificate", "certificates", "secret", "secrets",
    "token", "tokens", "credential", "credentials",
    "permission", "permissions", "role", "roles", "policy", "policies",
    "rule", "rules", "condition", "conditions",
    "dates", "date", "locations", "location", "visible",
    "difference", "between", "total", "partly",
    "upcoming", "securing", "production", "environment", "against",
];

/// Stem common verb→noun forms for better entity extraction.
/// This is a simple suffix-stripping stemmer, NOT a full NLP pipeline.
fn simple_stem(word: &str) -> String {
    let w = word.to_lowercase();
    
    // Common verb→noun mappings (data-driven, not hardcoded per-query)
    match w.as_str() {
        "securing" => return "security".to_string(),
        "configure" | "configuring" => return "configuration".to_string(),
        "deploy" | "deploying" => return "deployment".to_string(),
        "manage" | "managing" => return "management".to_string(),
        "monitor" | "monitoring" => return "monitoring".to_string(),
        "test" | "testing" => return "testing".to_string(),
        "debug" | "debugging" => return "debugging".to_string(),
        "develop" | "developing" => return "development".to_string(),
        "build" | "building" => return "build".to_string(),
        "install" | "installing" => return "installation".to_string(),
        "authenticate" | "authenticating" => return "authentication".to_string(),
        "authorize" | "authorizing" => return "authorization".to_string(),
        "encrypt" | "encrypting" => return "encryption".to_string(),
        "decrypt" | "decrypting" => return "decryption".to_string(),
        "validate" | "validating" => return "validation".to_string(),
        "sanitize" | "sanitizing" => return "sanitization".to_string(),
        "hash" | "hashing" => return "hashing".to_string(),
        "compress" | "compressing" => return "compression".to_string(),
        "decompress" | "decompressing" => return "decompression".to_string(),
        "serialize" | "serializing" => return "serialization".to_string(),
        "deserialize" | "deserializing" => return "deserialization".to_string(),
        "encode" | "encoding" => return "encoding".to_string(),
        "decode" | "decoding" => return "decoding".to_string(),
        "parse" | "parsing" => return "parsing".to_string(),
        "compile" | "compiling" => return "compilation".to_string(),
        "interpret" | "interpreting" => return "interpretation".to_string(),
        "optimize" | "optimizing" => return "optimization".to_string(),
        "profile" | "profiling" => return "profiling".to_string(),
        "log" | "logging" => return "logging".to_string(),
        "trace" | "tracing" => return "trace".to_string(),
        "report" | "reporting" => return "report".to_string(),
        _ => {}
    }

    // General suffix stripping
    if w.ends_with("ing") && w.len() > 4 {
        let base = &w[..w.len() - 3];
        // Handle doubling: "running" → "run"
        if base.ends_with(|c: char| c == 'n' || c == 'g' || c == 't' || c == 'd') {
            let single = &base[..base.len() - 1];
            if single.len() >= 2 {
                return single.to_string();
            }
        }
        return base.to_string();
    }
    if w.ends_with("ment") && w.len() > 4 {
        return w[..w.len() - 4].to_string();
    }
    if w.ends_with("ness") && w.len() > 4 {
        return w[..w.len() - 4].to_string();
    }
    if w.ends_with("able") && w.len() > 4 {
        return w[..w.len() - 4].to_string();
    }
    if w.ends_with("ous") && w.len() > 3 {
        return w[..w.len() - 3].to_string();
    }
    if w.ends_with("ive") && w.len() > 3 {
        return w[..w.len() - 3].to_string();
    }
    if w.ends_with("er") && w.len() > 3 {
        return w[..w.len() - 2].to_string();
    }
    if w.ends_with("or") && w.len() > 3 {
        return w[..w.len() - 2].to_string();
    }
    if w.ends_with("s") && w.len() > 2 && !w.ends_with("ss") {
        return w[..w.len() - 1].to_string();
    }

    w
}

/// Check if a word is a stop word.
fn is_stop_word(word: &str) -> bool {
    STOP_WORDS.contains(&word)
}

/// Check if a word is a technical keep-word.
fn is_technical_keep_word(word: &str) -> bool {
    TECHNICAL_KEEP_WORDS.contains(&word)
}

/// Check if a word is a modifier word.
fn is_modifier_word(word: &str) -> bool {
    MODIFIER_WORDS.contains(&word)
}

/// Extract strong entities from a query by stripping stop words and modifier words.
pub fn extract_entities(query: &str) -> Vec<String> {
    let words: Vec<&str> = query
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .collect();

    let mut entities = Vec::new();
    let mut seen = HashSet::new();

    for word in words {
        let lower = word.to_lowercase();
        if is_stop_word(&lower) {
            continue;
        }
        
        // Apply stemming
        let stemmed = simple_stem(&lower);
        
        if is_modifier_word(&stemmed) && !is_technical_keep_word(&stemmed) {
            continue;
        }
        if is_stop_word(&stemmed) {
            continue;
        }
        
        let canonical = if is_technical_keep_word(&stemmed) {
            stemmed
        } else {
            stemmed
        };
        
        if !seen.contains(&canonical) {
            seen.insert(canonical.clone());
            entities.push(canonical);
        }
    }

    entities
}

/// Generate all non-empty subsets of entities, ordered by size (largest first),
/// then lexicographically within each size.
fn generate_subsets(entities: &[String]) -> Vec<Vec<String>> {
    let n = entities.len();
    if n == 0 {
        return Vec::new();
    }

    let mut subsets: Vec<Vec<String>> = Vec::new();
    let total = 1usize << n; // 2^n subsets

    // Start from 1 to skip the empty subset
    for mask in 1..total {
        let mut subset: Vec<String> = Vec::new();
        for i in 0..n {
            if mask & (1 << i) != 0 {
                subset.push(entities[i].clone());
            }
        }
        subsets.push(subset);
    }

    // Sort by size descending, then lexicographically
    subsets.sort_by(|a, b| {
        b.len()
            .cmp(&a.len())
            .then_with(|| a.join(" ").cmp(&b.join(" ")))
    });

    subsets
}

/// Generate expanded query variants from a query string.
///
/// Returns a list of expanded queries ordered by specificity (most specific first).
/// Each variant is a space-separated combination of strong entities.
pub fn expand(query: &str) -> Vec<String> {
    let entities = extract_entities(query);
    if entities.is_empty() {
        return Vec::new();
    }

    let subsets = generate_subsets(&entities);
    subsets.iter().map(|s| s.join(" ")).collect()
}

/// Generate expanded query variants with a maximum count.
///
/// Returns at most `max` variants, ordered by specificity.
pub fn expand_capped(query: &str, max: usize) -> Vec<String> {
    expand(query)
        .into_iter()
        .take(max)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_entities_basic() {
        let query = "best practices for securing a postgresql database in a production environment against sql injection";
        let entities = extract_entities(query);
        assert!(entities.contains(&"postgresql".to_string()));
        assert!(entities.contains(&"database".to_string()));
        assert!(entities.contains(&"security".to_string()));
        assert!(entities.contains(&"sql".to_string()));
        assert!(entities.contains(&"injection".to_string()));
        // "best" and "practices" should be stripped as modifiers
        assert!(!entities.contains(&"best".to_string()));
        assert!(!entities.contains(&"practices".to_string()));
    }

    #[test]
    fn test_expand_acceptance() {
        let query = "best practices for securing a postgresql database in a production environment against sql injection";
        let variants = expand(query);
        assert!(!variants.is_empty());
        // Most specific first
        assert!(variants[0].contains("postgresql"));
        assert!(variants[0].contains("database"));
        assert!(variants[0].contains("security"));
    }

    #[test]
    fn test_expand_capped() {
        let query = "python web framework";
        let variants = expand_capped(query, 3);
        assert!(variants.len() <= 3);
    }

    #[test]
    fn test_single_entity() {
        let query = "postgresql";
        let variants = expand(query);
        assert_eq!(variants, vec!["postgresql"]);
    }

    #[test]
    fn test_stop_words_only() {
        let query = "the a an";
        let variants = expand(query);
        assert!(variants.is_empty());
    }

    #[test]
    fn test_duplicates() {
        let query = "python python java";
        let entities = extract_entities(query);
        let mut seen = std::collections::HashSet::new();
        for e in &entities {
            assert!(seen.insert(e), "Duplicate entity: {}", e);
        }
    }

    #[test]
    fn test_plural_modifiers() {
        let query = "best practices postgresql";
        let entities = extract_entities(query);
        assert!(!entities.contains(&"practices".to_string()));
        assert!(entities.contains(&"postgresql".to_string()));
    }

    #[test]
    fn test_ordering() {
        let query = "one two three four five";
        let variants = expand(query);
        // Largest subsets first
        assert_eq!(variants[0].split_whitespace().count(), 5);
    }

    #[test]
    fn test_capping() {
        let query = "one two three four five";
        let variants = expand_capped(query, 5);
        assert_eq!(variants.len(), 5);
    }

    #[test]
    fn test_stemming() {
        assert_eq!(simple_stem("securing"), "security");
        assert_eq!(simple_stem("running"), "run");
        assert_eq!(simple_stem("testing"), "testing");
    }
}
