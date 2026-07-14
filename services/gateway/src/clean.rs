// ─── Result Content Cleaning, URL Canonicalization, Query Classification ──
//
// This module holds the long-term robustness fixes for the search-result
// pipeline. Everything here is structural / phrase-pattern based — there are
// NO hardcoded domain allow/deny lists. The fixes address:
//
//  * ISSUE 1  Duplicate results — dedup by canonical URL (incl. web.archive.org
//               unwrapping) and by a content fingerprint so the same page
//               returned with different URLs/params is collapsed once.
//  * ISSUE 2  Empty / failed fetches — drop results whose content is empty,
//               a fetch-error string, or below a minimum information threshold.
//  * ISSUE 3  Boilerplate / nav leakage — strip HTML tags, raw CSS @font-face /
//               url(...) blocks, "toggle the table of contents" + language lists,
//               HTML5-audio fallbacks, photo-credit lines, Wikipedia citation
//               footnotes, and any other scraped-page noise from the snippet.
//  * ISSUE 8  Truncated mid-sentence / echoed title — bound the snippet to
//               complete sentences and strip a leading title echo.
//  * ISSUE 5/6/7 — query-type + required-entity classification used by the
//               ranking stage to bias result selection per intent.

use std::collections::HashSet;

/// Lightweight regex compile helper. Compiles on demand (results are few and
/// per-call cost is microseconds). Returns None on a (should-never-happen)
/// invalid pattern so the caller can safely skip the transform.
fn re(pattern: &str) -> Option<regex::Regex> {
    regex::Regex::new(pattern).ok()
}

/// Delete every top-level `{...}` JSON object that contains an `@context` or
/// `@type` key — i.e. embedded JSON-LD / structured-data blobs some engines
/// return as bare text (no `<script>` wrapper). A regex cannot match arbitrary
/// brace nesting, so this scans char-by-char, tracking brace depth, and drops a
/// balanced block once it is known to carry an LD+JSON marker. Braces inside
/// string literals (`"..."`) are ignored so `"a}b"` never fools the depth count.
/// Delete every {...} JSON object that contains an "@context" or "@type" key
/// i.e. embedded JSON-LD / structured-data blobs some engines return as bare
/// text (no <script> wrapper). A regex cannot match arbitrary brace nesting,
/// so this scans char-by-char and drops a brace-balanced block once it carries
/// an LD+JSON marker. The blob may appear at the top level OR inside a larger
/// quoted string of the snippet (e.g. "prefix {"@context":...} suffix"); both
/// are handled by descending into strings and stripping just the inner {...}
/// when it is LD+JSON. Schema.org blobs have no nested braces in their string
/// values, so a simple brace-depth count is sufficient.
fn strip_json_ld_blobs(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            // Copy the string literal, but strip any embedded JSON-LD {...}.
            out.push('"');
            let mut j = i + 1;
            while j < chars.len() {
                let sc = chars[j];
                if sc == '"' {
                    out.push('"');
                    i = j + 1;
                    break;
                }
                if sc == '{' {
                    // Brace-balanced scan of the embedded object.
                    let mut depth = 0usize;
                    let mut k = j;
                    while k < chars.len() {
                        if chars[k] == '{' {
                            depth += 1;
                        } else if chars[k] == '}' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        k += 1;
                    }
                    if k < chars.len() {
                        let block: String = chars[j..=k].iter().collect();
                        if !(block.contains("@context") || block.contains("@type")) {
                            out.push_str(&block);
                        }
                        j = k + 1;
                    } else {
                        // Unbalanced inside string (truncated snippet). Drop the
                        // rest if it is an LD+JSON fragment, else keep verbatim.
                        let rest: String = chars[j..].iter().collect();
                        if !(rest.contains("@context") || rest.contains("@type")) {
                            for ch in &chars[j..] {
                                out.push(*ch);
                            }
                        }
                        j = chars.len();
                    }
                    continue;
                }
                out.push(sc);
                j += 1;
            }
            continue;
        }
        if c == '{' {
            let mut depth = 0usize;
            let mut j = i;
            while j < chars.len() {
                if chars[j] == '{' {
                    depth += 1;
                } else if chars[j] == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                j += 1;
            }
            if j < chars.len() {
                let block: String = chars[i..=j].iter().collect();
                if block.contains("@context") || block.contains("@type") {
                    if !out.ends_with(' ') && j + 1 < chars.len() && chars[j + 1] != ' ' {
                        out.push(' ');
                    }
                } else {
                    out.push_str(&block);
                }
                i = j + 1;
            } else {
                // Unbalanced brace (truncated upstream snippet). If it is an
                // LD+JSON fragment, drop the rest; otherwise keep it verbatim.
                let rest: String = chars[i..].iter().collect();
                if !(rest.contains("@context") || rest.contains("@type")) {
                    out.push_str(&rest);
                }
                break;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}


/// Strip CSS rule blocks (which upstream scrapers sometimes embed verbatim
/// inside a snippet, e.g. "@media (max-width: 767px) { #_R_... iframe } Advertisement").
/// A single-level regex cannot reach nested-brace CSS, so this scans char-by-char with
/// a brace-depth counter (respecting "..." strings, /* */ comments, and (...) groups so
/// CSS like `content: "a}b"` or `url(http://a}b)` is not mis-counted). A {...} block is
/// removed only when it LOOKS like CSS, not prose: it must carry a CSS signature (an
/// @-rule, `url(`, a `prop:` declaration, an `iframe` token, etc.) AND must not contain a
/// genuine sentence clause. This guards against eating real sentences that sit in braces.
fn strip_css_blocks(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '{' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        // Find the matching close brace (depth-balanced; strings/comments/(...) respected).
        let mut depth = 0usize;
        let mut in_str = false;
        let mut in_comment = false;
        let mut end = None;
        let mut j = i;
        while j < chars.len() {
            let c = chars[j];
            if in_comment {
                if c == '*' && j + 1 < chars.len() && chars[j + 1] == '/' {
                    in_comment = false;
                    j += 2;
                    continue;
                }
            } else if in_str {
                if c == '\\' {
                    j += 2;
                    continue;
                } else if c == '"' {
                    in_str = false;
                }
            } else if c == '/' && j + 1 < chars.len() && chars[j + 1] == '*' {
                in_comment = true;
                j += 2;
                continue;
            } else if c == '"' {
                in_str = true;
            } else if c == '(' {
                // Skip a (...) group (e.g. url(http://a}b)) so a brace inside
                // parentheses does not perturb the brace-depth count.
                let mut pd = 1usize;
                let mut pk = j + 1;
                while pk < chars.len() {
                    if chars[pk] == '(' {
                        pd += 1;
                    } else if chars[pk] == ')' {
                        pd -= 1;
                        if pd == 0 {
                            break;
                        }
                    } else if chars[pk] == '"' {
                        pk += 1;
                        while pk < chars.len() && chars[pk] != '"' {
                            pk += 1;
                        }
                    }
                    pk += 1;
                }
                j = pk + 1;
                continue;
            } else if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth -= 1;
                if depth == 0 {
                    end = Some(j);
                    break;
                }
            }
            j += 1;
        }
        match end {
            None => {
                for ch in &chars[i..] {
                    out.push(*ch);
                }
                break;
            }
            Some(end) => {
                let block: String = chars[i..=end].iter().collect();
                if looks_like_css(&block) {
                    if !out.ends_with(' ') && end + 1 < chars.len() && chars[end + 1] != ' ' {
                        out.push(' ');
                    }
                } else {
                    out.push_str(&block);
                }
                i = end + 1;
            }
        }
    }
    out
}

/// Heuristic: does this {...} block look like CSS rather than prose?
/// True (strip it) when it carries a CSS *signature* AND has no genuine
/// sentence clause (we never eat a real sentence that happens to contain code).
fn looks_like_css(block: &str) -> bool {
    // Genuine prose guard: a real sentence clause means keep the block.
    if block.contains(". ") || block.contains("? ") || block.contains("! ") {
        return false;
    }
    let lower = block.to_lowercase();
    let has_at_rule = lower.contains("@media")
        || lower.contains("@font-face")
        || lower.contains("@import")
        || lower.contains("@keyframes")
        || lower.contains("@charset");
    let has_other_marker = lower.contains("!important")
        || lower.contains("url(")
        || lower.contains("iframe")
        || lower.contains("@font");
    // Property declaration shape: a letter before ":" not part of a URL scheme
    // (http:/https: or //). e.g. "color:", "background:", "width:".
    let mut has_prop = false;
    let chars: Vec<char> = block.chars().collect();
    for k in 0..chars.len() {
        if chars[k] == ':' {
            let prev_is_alpha = k > 0 && chars[k - 1].is_alphabetic();
            let next_bad = k + 1 < chars.len() && chars[k + 1] == '/';
            if prev_is_alpha && !next_bad {
                has_prop = true;
                break;
            }
        }
    }
    has_at_rule || has_other_marker || has_prop
}

/// Strip HTML tags, raw CSS / JS blocks, and scraped-page boilerplate from a
/// search-engine content snippet, then bound it to complete sentences and
/// remove a leading title echo. Returns a cleaned string (may be empty if the
/// source was pure boilerplate).
pub fn clean_result_content(raw: &str, title: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let mut s = raw.to_string();

    // 1. Strip <style>/<script> blocks and raw CSS rule blocks + url(...) refs.
    if let Some(r) = re(r"(?is)<style[^>]*>.*?</style>") {
        s = r.replace_all(&s, " ").into_owned();
    }
    if let Some(r) = re(r"(?is)<script[^>]*>.*?</script>") {
        s = r.replace_all(&s, " ").into_owned();
    }
    // Strip embedded JSON-LD / structured-data blobs (e.g.
    // {"@context":"https://...","@type":"BreadcrumbList","itemListElement":[...]}).
    // Some engines return the parsed LD+JSON *text* without its <script> wrapper,
    // so the generic <script> strip above misses it. A regex can't match arbitrary
    // brace nesting, so strip recursively by deleting each {...} block that contains
    // an "@context" or "@type" key (brace-depth balanced).
    s = strip_json_ld_blobs(&s);
    // Strip nested-brace CSS rule blocks (e.g. "@media (max-width: 767px) { #_R_...
    // iframe } Advertisement"). This brace-balanced scanner supersedes the old
    // single-level regexes below (which mis-count braces inside url(...) and nested
    // rules), so it runs first; the legacy strips remain as a cheap safety net.
    s = strip_css_blocks(&s);
    // CSS: nested brace blocks, then single brace blocks.
    if let Some(r) = re(r"(?s)\{[^{}]*\{[^{}]*\}[^{}]*\}") {
        s = r.replace_all(&s, " ").into_owned();
    }
    if let Some(r) = re(r"(?s)\{[^{}]*\}") {
        s = r.replace_all(&s, " ").into_owned();
    }
    if let Some(r) = re(r#"(?i)url\(\s*['"]?[^"')]*['"]?\s*\)"#) {
        s = r.replace_all(&s, " ").into_owned();
    }
    for kw in ["@font-face", "@media", "@import", "@charset", "@keyframes"] {
        if let Some(r) = re(&format!(r"(?i){}", regex::escape(kw))) {
            s = r.replace_all(&s, " ").into_owned();
        }
    }

    // 2. Strip HTML tags (any leftover <...>).
    if let Some(r) = re(r"(?s)<[^>]+>") {
        s = r.replace_all(&s, " ").into_owned();
    }

    // 3. Decode the common HTML entities SearXNG leaves behind.
    s = s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#039;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&#x27;", "'")
        .replace("&#x2F;", "/")
        .replace("&rsquo;", "'")
        .replace("&lsquo;", "'")
        .replace("&ldquo;", "\"")
        .replace("&rdquo;", "\"")
        .replace("&hellip;", "…")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–");

    // 4. Remove scraped boilerplate / nav / artifact phrases.
    // Wikipedia "Toggle the table of contents" + the following language list.
    if let Some(r) = re(r"(?i)toggle the table of contents[^\n]{0,700}") {
        s = r.replace_all(&s, " ").into_owned();
    }
    // HTML5 audio fallback lines.
    if let Some(r) = re(r"(?i)your browser doesn't support html5 audio[^\n]*") {
        s = r.replace_all(&s, " ").into_owned();
    }
    // IPA / phonetic notation like /ˈprez.ɪ.d ə nt/ us  or  / tel / us
    if let Some(r) = re(r"(?i)[/\[]\s*[\p{L}\p{M}.ˈˌ: ]{2,}\s*[/\]]\s*(uk|us)?") {
        s = r.replace_all(&s, " ").into_owned();
    }
    // Photo-credit lines.
    if let Some(r) = re(r"(?i)photograph:[^\n]*?(view image in fullscreen|view image|getty images?|via [a-z]+|/ap|\b(ap|reuters|pa media|epa|afp)\b)") {
        s = r.replace_all(&s, " ").into_owned();
    }
    if let Some(r) = re(r"(?i)view image in fullscreen") {
        s = r.replace_all(&s, " ").into_owned();
    }
    // Newsletter / signup promos ("Sign up now!", "our daily newsletter", etc.).
    if let Some(r) = re(r"(?i)\b(sign up (now|today|free|here)|subscribe (now|today|to (our|the))|our (daily|weekly|free) newsletter|\w+ daily newsletter|get the \w+ newsletter)[^\n]*") {
        s = r.replace_all(&s, " ").into_owned();
    }
    if let Some(r) = re(r"(?i)image credit:[^\n]*") {
        s = r.replace_all(&s, " ").into_owned();
    }
    // Wikipedia / reference citation footnotes:
    //   "Sanders, Emma (18 December 2022). "World Cup...". BBC Sport. Retrieved 25..."
    if let Some(r) = re(r#"(?i)[A-Z][\w.'-]+(, [A-Z][\w.'-]+)*\s*\(\s*\d{1,2}\s+\w+\s+\d{4}\s*\)\.\s+["“]"#) {
        s = r.replace_all(&s, " ").into_owned();
    }
    if let Some(r) = re(r"(?i)retrieved\s+\d{1,2}\s+\w+\s+\d{4}[^\n]*") {
        s = r.replace_all(&s, " ").into_owned();
    }
    // Dictionary.com "Jump To:" nav.
    if let Some(r) = re(r"(?i)jump to:[^\n]*") {
        s = r.replace_all(&s, " ").into_owned();
    }
    // Generic "Read more", "See also", cookie/consent leftovers.
    if let Some(r) = re(r"(?i)\b(read more|see also|cookie policy|accept cookies|we use cookies)[^\n]*") {
        s = r.replace_all(&s, " ").into_owned();
    }
    // Ad-label boilerplate that scrapers sometimes leave dangling after a stripped
    // CSS rule block (e.g. "...iframe } Advertisement #_R_...").
    if let Some(r) = re(r"(?i)\badvertisement\b") {
        s = r.replace_all(&s, " ").into_owned();
    }
    // Publisher "about this article" footer taglines that carry no content
    // (e.g. The Guardian's standard "Just an example of the laughs we have here.").
    if let Some(r) = re(r"(?i)just an example of the laughs we have here[.]?") {
        s = r.replace_all(&s, " ").into_owned();
    }
    // The Verge-style engagement boilerplate.
    if let Some(r) = re(r"(?i)posts from this topic will be added to your daily email digest[^\n]*") {
        s = r.replace_all(&s, " ").into_owned();
    }
    if let Some(r) = re(r"(?i)follow follow see all[^\n]*") {
        s = r.replace_all(&s, " ").into_owned();
    }
    if let Some(r) = re(r"(?i)report close report[^\\n]*") {
        s = r.replace_all(&s, " ").into_owned();
    }
    // Internet Archive / video-placeholder boilerplate ("Video Item Preview",
    // "in-browser video \"theater\" requires JavaScript to be enabled", etc.).
    if let Some(r) = re(r#"(?i)(video item preview|in-browser video [“”]?theater[“”]? requires javascript|this item does not appear to have any files)[^
]*"#) {
        s = r.replace_all(&s, " ").into_owned();
    }

    // 5. Collapse runs of whitespace into single spaces.
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for c in s.chars() {
        if c == '\n' || c == '\r' || c == '\t' || c == ' ' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
    }
    let mut s = out.trim().to_string();

    // 6. Strip a leading echo of the title.
    s = strip_title_echo(&s, title);

    // 7. Bound to complete sentences (drop a trailing fragment).
    s = bound_to_sentences(&s);

    s
}

/// Remove a leading prefix of `content` that repeats the result `title`.
/// e.g. "Capital of Japan The capital of Japan is Tokyo." -> "The capital of
/// Japan is Tokyo."
pub fn strip_title_echo(content: &str, title: &str) -> String {
    let c = content.trim_start();
    let t = title.trim();
    // Work in CHARACTER space, never byte space, so multibyte chars (’ “ —)
    // never split a codepoint and panic.
    let c_chars: Vec<char> = c.chars().collect();
    let t_chars: Vec<char> = t.chars().collect();
    if c_chars.is_empty() || t_chars.is_empty() {
        return content.to_string();
    }
    // Find the length L of the longest common prefix (case-insensitive) between
    // content and title. We strip an echo when content *begins* with a
    // meaningful leading portion of the title and then continues with body text
    // (the page's own content). This catches both:
    //   * content starts with the FULL title, OR
    //   * content starts with a PREFIX of the title (e.g. the real title is
    //     "X -- Site Name" but the content only echoed "X " before the body).
    let c_low: Vec<char> = c_chars.iter().map(|ch| ch.to_ascii_lowercase()).collect();
    let t_low: Vec<char> = t_chars.iter().map(|ch| ch.to_ascii_lowercase()).collect();
    let mut l = 0;
    while l < c_chars.len() && l < t_chars.len() && c_low[l] == t_low[l] {
        l += 1;
    }
    // Require a non-trivial shared prefix (avoid stripping on a 3-char coincidence).
    if l >= 15 {
        // The ENTIRE content is just the title (no body follows) — it is a pure
        // title echo with no usable text. Return empty so the caller drops it.
        if l >= c_chars.len() {
            return String::new();
        }
        // Content continues beyond the shared prefix (there is real body text).
        // Allow the shared prefix to end at a word boundary or a separator char;
        // trim any trailing separator/space before the body starts.
        let rest: String = c_chars[l..].iter().collect();
        let rest_trim = rest.trim_start();
        let rest_stripped = rest_trim
            .strip_prefix(['-', '|', ':', '–', '—', ' '])
            .unwrap_or(rest_trim)
            .trim_start();
        return rest_stripped.to_string();
    }
    content.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_title_echo_pure_title() {
        // Content is exactly the title — a pure echo with no body. Must drop it.
        let title = "How To Tell If Someone Is A Pathological Liar : Sam Vaknin";
        let content = "How To Tell If Someone Is A Pathological Liar";
        let out = strip_title_echo(content, title);
        assert!(out.is_empty(), "pure-title echo not emptied: {out:?}");
    }

    #[test]
    fn strip_title_echo_exact_prefix() {
        let title = "What happened to the fight for the Internet?";
        let content = "What happened to the fight for the Internet? By Christine Lemmer-Webber on Tue 30 June 2026 At the moment I am writing this, bad internet bills are being proposed across the US, Canada, Europe, and th";
        let out = strip_title_echo(content, title);
        assert!(
            !out.starts_with("What happened to the fight for the Internet?"),
            "echo not stripped: {out:?}"
        );
        assert!(
            out.starts_with("By Christine"),
            "unexpected remainder: {out:?}"
        );
    }

    #[test]
    fn strip_title_echo_title_dash_sep() {
        let title = "What Happened in 1923";
        let content = "What Happened in 1923 - On This Day In 1923, important events...";
        let out = strip_title_echo(content, title);
        // Echo (title + " - ") is stripped; remainder begins with the real sentence.
        assert!(out.starts_with("On This Day"), "unexpected: {out:?}");
    }

    // Real-world case: the indexed title carries a " -- Site Name" suffix that the
    // content did NOT echo, so content starts with only a PREFIX of the title.
    #[test]
    fn strip_title_echo_title_prefix_only() {
        let title = "What happened to the fight for the Internet? -- Dustycloud Brainstorms";
        let content = "What happened to the fight for the Internet? By Christine Lemmer-Webber on Tue 30 June 2026 At the moment I am writing this, bad internet bills...";
        let out = strip_title_echo(content, title);
        assert!(
            !out.starts_with("What happened to the fight for the Internet?"),
            "prefix echo not stripped: {out:?}"
        );
        assert!(out.starts_with("By Christine"), "unexpected: {out:?}");
    }

    #[test]
    fn clean_removes_css_and_html() {
        let dirty = "Body text here <img alt=\"x\"> and <div>more</div> @font-face{font-family:test} url(http://a.com/b.png) tail";
        let out = clean_result_content(dirty, "");
        assert!(!out.contains("<img"), "html tag left: {out}");
        assert!(!out.contains("@font-face"), "css left: {out}");
        assert!(!out.contains("url("), "css url left: {out}");
    }

    #[test]
    fn clean_strips_json_ld_blob() {
        // Engines sometimes return parsed LD+JSON *text* (no <script> wrapper).
        // Real example seen in live results: a BreadcrumbList blob leaking into the snippet.
        // The lead text ("Find the") is deliberately not the title so the title-echo
        // stripper doesn't remove it.
        let dirty = "Find the Best Books 2024 {\"@context\":\"https://***@type\":\"BreadcrumbList\",\"itemListElement\":[{\"@type\":\"ListItem\",\"position\":1,\"name\":\"Home\"}]} Buy now";
        let out = clean_result_content(dirty, "Best Books 2024");
        assert!(!out.contains("@context"), "json-ld @context left: {out}");
        assert!(!out.contains("@type"), "json-ld @type left: {out}");
        assert!(out.contains("Find the Best Books 2024"), "real text dropped: {out}");
        assert!(out.contains("Buy now"), "trailing text dropped: {out}");
    }

    #[test]
    fn clean_strips_json_ld_deeply_nested() {
        // Deeper nesting (itemListElement -> item -> @type) that a single-level
        // regex cannot fully consume. The brace-balanced stripper must remove it all.
        let dirty = "Read this {\"@context\":\"https://***@graph\":[{\"@type\":\"Book\",\"name\":\"X\",\"author\":{\"@type\":\"Person\",\"name\":\"Y\"}}]} end";
        let out = clean_result_content(dirty, "Read this");
        assert!(!out.contains("@context"), "deep @context left: {out}");
        assert!(!out.contains("@type"), "deep @type left: {out}");
        assert!(out.contains("Read this"), "lead text dropped: {out}");
        assert!(out.contains("end"), "trail text dropped: {out}");
    }

    #[test]
    fn clean_strips_json_ld_inside_string() {
        // Real live shape: the LD+JSON blob is embedded inside a larger quoted
        // string of the snippet, e.g.  "prefix {"@context":...} suffix".
        // The lead text ("Check this") is not the title so the title-echo
        // stripper doesn't remove it.
        let dirty = "Check this \"Related: {\"@context\":\"https://***@type\":\"BlogPosting\",\"headline\":\"X\"} more here\" thanks";
        let out = clean_result_content(dirty, "BlogPosting X");
        assert!(!out.contains("@context"), "json-ld @context left: {out}");
        assert!(!out.contains("@type"), "json-ld @type left: {out}");
        assert!(out.contains("Check this"), "lead dropped: {out}");
        assert!(out.contains("more here"), "string-surrounding text dropped: {out}");
        assert!(out.contains("thanks"), "trail dropped: {out}");
    }

    #[test]
    fn clean_strips_json_ld_truncated() {
        // Upstream sometimes truncates the LD+JSON blob mid-value, leaving an
        // unbalanced "{" with no closing brace. It must still be dropped.
        let dirty = "Intro text {\"@context\":\"https://schema.";
        let out = clean_result_content(dirty, "Intro text");
        assert!(!out.contains("@context"), "truncated json-ld left: {out}");
        assert!(out.contains("Intro text"), "lead dropped: {out}");
    }

    #[test]
    fn clean_strips_nested_css_media_iframe() {
        // Real live shape: a CSS media-query rule leaked into the snippet, with the
        // iframe token inside a NESTED brace. A single-level regex cannot reach it.
        let dirty = "Video Science & Exploration 8 th width: 767.95px) { #_R_29avcqbsnqq5b_ iframe } Advertisement #_R_49avcqbsnqq5b tail";
        // The scanner itself must remove the brace block (regression-proof unit).
        let stripped = strip_css_blocks(dirty);
        assert!(!stripped.contains("iframe"), "css iframe block left: {stripped}");
        assert!(stripped.contains("tail"), "trail dropped: {stripped}");
        // End-to-end through the full cleaner (also strips the dangling ad-label).
        let out = clean_result_content(dirty, "Video Science");
        assert!(!out.contains("iframe"), "css iframe block left: {out}");
        assert!(!out.contains("Advertisement"), "css advert block left: {out}");
    }

    #[test]
    fn clean_css_brace_counter_ignores_urls() {
        // A url(...) containing a "}" must not break the brace-depth counter.
        let dirty = "Head { background: url(http://a}b) center; color: red } Tail text";
        let out = clean_result_content(dirty, "Head");
        assert!(!out.contains("url("), "css block left: {out}");
        assert!(!out.contains("color:"), "css prop left: {out}");
        assert!(out.contains("Head"), "lead dropped: {out}");
        assert!(out.contains("Tail text"), "trail dropped: {out}");
    }

    #[test]
    fn clean_keeps_brace_prose() {
        // Regression guard: a {...} block holding a real English sentence must be
        // PRESERVED by the CSS scanner (not stripped as if it were CSS).
        let dirty = "Before { This is a real sentence about cats. } After";
        let out = strip_css_blocks(dirty);
        assert!(out.contains("This is a real sentence about cats."), "prose inside braces dropped: {out}");
        assert!(out.contains("After"), "trail dropped: {out}");
    }

    #[test]
    fn canonicalize_unwraps_archive_plain() {
        assert_eq!(
            canonicalize_url("https://web.archive.org/web/20200101/https://example.com/page"),
            "example.com/page"
        );
    }

    #[test]
    fn canonicalize_unwraps_archive_pct() {
        assert_eq!(
            canonicalize_url("https://web.archive.org/web/3/https%3A%2F%2Fexample.com%2Fpage"),
            "example.com/page"
        );
    }

    #[test]
    fn is_junk_catches_fetch_error() {
        assert!(is_junk_content(
            "We cannot provide a description for this page right now"
        ));
    }
}


/// Keep only complete sentences: drop any trailing fragment that does not end
/// in terminal punctuation. A snippet that is purely a fragment (no terminal
/// punctuation anywhere) is returned trimmed — the caller decides if it is
/// discardable.
pub fn bound_to_sentences(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // Find the last terminal punctuation. If everything after it is whitespace,
    // the snippet ends on a full sentence — keep it as-is.
    if let Some(pos) = trimmed.rfind(|c| c == '.' || c == '!' || c == '?') {
        let after = &trimmed[pos + 1..];
        if after.trim().is_empty() {
            return trimmed.to_string();
        }
        // There is trailing text after the last sentence — cut it off so we
        // never emit a mid-clause fragment.
        return trimmed[..=pos].trim().to_string();
    }
    trimmed.to_string()
}

/// True when the (already-cleaned) content carries no usable information and
/// should be dropped before ranking/return.
pub fn is_junk_content(content: &str) -> bool {
    let c = content.trim();
    if c.is_empty() {
        return true;
    }
    let cl = c.to_lowercase();
    // Fetch-error strings returned by some engines.
    let fetch_errors = [
        "we cannot provide a description for this page right now",
        "we can't provide a description for this page right now",
        "cannot provide a description for this page right now",
        "we couldn't find a description for this page",
    ];
    if fetch_errors.iter().any(|e| cl.contains(e)) {
        return true;
    }
    // Boilerplate that survived cleaning (e.g. a TOC that exceeded the strip window).
    if cl.contains("toggle the table of contents") {
        return true;
    }
    // Internet Archive / video-placeholder boilerplate that yields no real text.
    if cl.contains("in-browser video") && cl.contains("requires javascript") {
        return true;
    }
    if cl.contains("video item preview") {
        return true;
    }
    // Minimum information threshold: fewer than 15 alphanumeric chars is noise.
    let alnum = c.chars().filter(|ch| ch.is_alphanumeric()).count();
    if alnum < 15 {
        return true;
    }
    false
}

/// Canonicalize a URL for dedup: lowercase host+path, strip trailing slash,
/// and unwrap web.archive.org (and similar) wrappers to the underlying URL.
pub fn canonicalize_url(url: &str) -> String {
    // Unwrap web.archive.org (plain):  https://web.archive.org/web/<ts>/<realurl>
    if let Some(r) = re(r"(?i)https?://web\.archive\.org/web/\d+(?:[a-z_]*id_?)?/(https?://.*)") {
        if let Some(caps) = r.captures(url) {
            if let Some(real) = caps.get(1) {
                return canonicalize_url(real.as_str());
            }
        }
    }
    // Unwrap web.archive.org (percent-encoded):  .../web/3/https%3A%2F%2F<realurl>
    if let Some(r) = re(r"(?i)https?://web\.archive\.org/web/\d+(?:[a-z_]*id_?)?/(https?%3a%2f%2f.*)") {
        if let Some(caps) = r.captures(url) {
            if let Some(real) = caps.get(1) {
                // Percent-decode the captured inner URL before recursing.
                if let Ok(decoded) = percent_decode(real.as_str()) {
                    return canonicalize_url(&decoded);
                }
            }
        }
    }
    // Unwrap other common archive wrappers (archive.today, t.co-style shorteners
    // are not reversible, so only handle the structural ones).
    if let Some(parsed) = reqwest::Url::parse(url).ok() {
        let host = parsed.host_str().unwrap_or("").to_lowercase();
        let path = parsed.path().to_lowercase().trim_end_matches('/').to_string();
        return format!("{}{}", host, path);
    }
    url.to_lowercase()
}

/// Minimal percent-decoder (only for ASCII-ish URL payloads we hit in archives).
fn percent_decode(s: &str) -> Result<String, std::string::FromUtf8Error> {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out)
}

/// A stable fingerprint of a result's (title + content) used to collapse
/// exact-duplicate pages that differ only by URL.
pub fn content_fingerprint(title: &str, content: &str) -> String {
    let t: String = title
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(60)
        .collect();
    let c: String = content
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(180)
        .collect();
    format!("{}|{}", t, c)
}

// ─── Query-type + entity classification (used by the ranking stage) ───

/// Coarse query-type classification driving result selection bias.
///   joke | who_won | factual_person | definition | factual_thing
///   | news_event | general
pub fn classify_query_type(query: &str) -> String {
    let q = query.to_lowercase();
    if q.contains("tell me a joke") || q.contains("joke") || q.contains("jokes") {
        return "joke".to_string();
    }
    if q.starts_with("who won")
        || q.starts_with("who win")
        || q.contains("winner of")
        || q.contains("who were the winners")
    {
        return "who_won".to_string();
    }
    if q.starts_with("who is")
        || q.starts_with("who was")
        || q.starts_with("who were")
    {
        return "factual_person".to_string();
    }
    if q.starts_with("what is")
        || q.starts_with("what are")
        || q.starts_with("what was")
        || q.starts_with("what were")
        || q.starts_with("define")
        || q.contains("definition of")
        || q.contains("meaning of")
    {
        return "definition".to_string();
    }
    if q.starts_with("what happened") {
        return "news_event".to_string();
    }
    if q.starts_with("capital of")
        || q.starts_with("where is")
        || q.starts_with("how many")
        || q.starts_with("how much")
    {
        return "factual_thing".to_string();
    }
    "general".to_string()
}

/// The distinctive entities a result MUST mention to be considered on-topic.
/// Filters out stop words and generic query scaffolding so the extracted
/// entities are the real subject (e.g. "what happened in 1923" -> ["1923"],
/// "who is the president of france" -> ["france"]).
pub fn required_entities(query: &str) -> Vec<String> {
    let stop: HashSet<&str> = [
        "the", "a", "an", "in", "on", "for", "of", "to", "with", "from", "is",
        "are", "was", "were", "who", "what", "when", "where", "why", "how", "do",
        "does", "did", "will", "would", "could", "should", "can", "may", "might",
        "me", "my", "your", "he", "she", "they", "it", "this", "that", "these",
        "those", "has", "have", "had", "been", "being", "also", "about", "into",
        "than", "then", "there", "here", "not", "no", "without", "except",
        "other", "president", "prime", "minister", "capital", "explain",
        "learn", "learning", "best", "top", "happened", "happen", "tell",
    ]
    .iter()
    .copied()
    .collect();
    query
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| {
            w.len() >= 2 && !stop.contains(w.as_str()) && !w.chars().all(|c| c.is_ascii_digit())
        })
        .collect()
}

/// True when the URL path/host indicates a time-sensitive news article — used
/// to demote news for encyclopedic ("what is X" / "who is X") queries.
pub fn is_news_path(url: &str) -> bool {
    let u = url.to_lowercase();
    u.contains("/news/")
        || u.contains("/world/")
        || u.contains("/politics/")
        || u.contains("/sport/")
        || u.contains("/business/")
        || u.contains("/election")
        || u.contains("/elections/")
        || u.contains("news.")
        || u.contains("/world-cup")
}

/// True if a result is a joke/aggregator listicle (the right answer for
/// "tell me a joke"), detected structurally — NOT by hardcoded domain.
pub fn is_listicle(title: &str, content: &str, url: &str) -> bool {
    let t = title.to_lowercase();
    let u = url.to_lowercase();
    let c = content.to_lowercase();
    // Numbered-list titles: "150 Hilariously Funny Jokes", "200 Corny Jokes".
    let re_num = re(r"(?i)\b(\d{2,4})\+?\s+(funny|hilarious|corny|best|top|clean|short|good)\s+\w*\s*(joke|pun|riddle|meme|quotes?|facts?|things|ways|reasons)\b");
    let has_numbered_title = re_num.map(|r| r.is_match(&t)).unwrap_or(false);
    // Listicle URL/markers: /jokes/, /tag/joke, "joke list", "knock knock".
    let listicle_marker = u.contains("/joke")
        || u.contains("jokes")
        || u.contains("pun")
        || u.contains("riddle")
        || c.contains("knock knock")
        || c.contains("why did the");
    // A title that is itself a single joke setup ("why did the chicken...").
    let is_joke_setup = (t.contains("why did the") || t.contains("knock knock") || t.contains("what do you call"))
        && t.split_whitespace().count() < 20;
    has_numbered_title || (listicle_marker && (t.contains("joke") || t.contains("pun") || t.contains("riddle"))) || is_joke_setup
}

/// True if a result is a dictionary/glossary definition page, detected by
/// content structure (phonetic notation, part-of-speech labels, single-word
/// title) — NOT by hardcoded domain. Used to demote dictionary matches for
/// non-dictionary queries (e.g. "tell me a joke", "what is dark matter").
pub fn is_definition_site(title_lc: &str, content_lc: &str) -> bool {
    let title_words: Vec<&str> = title_lc.split_whitespace().collect();
    let content_prefix: String = content_lc.chars().take(300).collect();
    let has_phonetic = content_prefix.contains("/ˈ")
        || content_prefix.contains("/ˌ")
        || content_prefix.contains("/'")
        || content_prefix.contains("/-");
    let has_pos_label = content_prefix.starts_with("noun")
        || content_prefix.starts_with("verb")
        || content_prefix.starts_with("adjective")
        || content_prefix.starts_with("adverb")
        || content_prefix.starts_with("preposition")
        || content_prefix.starts_with("conjunction")
        || content_prefix.starts_with("interjection")
        || content_prefix.starts_with("pronoun")
        || content_prefix.starts_with("determiner")
        || content_prefix.starts_with("abbreviation");
    let content_is_short = content_lc.len() < 200;
    let short_title = title_words.len() <= 3;
    (has_phonetic || has_pos_label) && short_title
        || has_pos_label && content_is_short
        || has_phonetic && short_title
}
