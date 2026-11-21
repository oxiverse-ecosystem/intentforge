// ─── Goals Feature Module ──────────────────────────────────────────────
// Takes a user's goal (e.g. "develop an AI assistant"), asks questions,
// uses intent engine for classification, search API for resource curation,
// and generates a personalized phased roadmap with deadlines + buffers.
//
// Endpoints:
//   POST /goals                   — Create a goal, get questions back
//   POST /goals/:id/answers       — Submit answers, get full roadmap
//   GET  /goals/:id               — Get goal status and roadmap
//   GET  /goals/leaderboard       — Get leaderboard
//   POST /goals/quick             — Quick one-shot: goal → full roadmap immediately
//   POST /goals/:id/phases/:phase_id/complete — Mark a phase as completed
//   POST /goals/:id/progress      — Update phase completion progress & score

use axum::{
    async_trait,
    extract::{FromRequest, Path, Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};

use serde::{Deserialize, Serialize};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ─── Custom Axum JSON Extractor ────────────────────────────────────
// Catches JSON deserialization rejections (e.g. missing fields, invalid JSON)
// and returns consistent 422 JSON error responses instead of plain-text.

pub struct AppJson<T>(pub T);

#[async_trait]
impl<S, T> FromRequest<S> for AppJson<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(value) => Ok(Self(value.0)),
            Err(rejection) => {
                let err_msg = rejection.body_text();
                let json_response = serde_json::json!({
                    "error": "invalid_payload",
                    "message": format!("Invalid or missing fields in request body: {}", err_msg)
                });
                Err((StatusCode::UNPROCESSABLE_ENTITY, Json(json_response)).into_response())
            }
        }
    }
}

// ─── Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalRequest {
    pub goal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub id: usize,
    pub question: String,
    pub description: String,
    pub options: Vec<String>,
    #[serde(rename = "type")]
    pub question_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerSubmission {
    pub answers: Vec<UserAnswer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAnswer {
    pub question_id: usize,
    pub answer: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseProgressRequest {
    pub phase_id: usize,
    pub is_completed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub title: String,
    pub url: String,
    pub resource_type: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalResourceCache {
    pub intent: String,
    pub resources: Vec<Resource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase {
    pub id: usize,
    pub title: String,
    pub description: String,
    pub duration_weeks: u32,
    pub deadline: String,
    pub buffer_days: u32,
    pub objectives: Vec<String>,
    pub resources: Vec<Resource>,
    pub deliverables: Vec<String>,
    pub completion_type: String,
    pub is_completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Roadmap {
    pub title: String,
    pub overview: String,
    pub phases: Vec<Phase>,
    pub total_phases: usize,
    pub total_duration_weeks: u32,
    pub total_buffer_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredGoal {
    pub goal_id: String,
    pub goal: String,
    pub intent: String,
    pub resources: Vec<Resource>,
    pub roadmap: Option<Roadmap>,
    pub created_at: String,
    pub completed_phases: usize,
    pub total_phases: usize,
    pub score: u32,
    pub user_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub goal_id: String,
    pub goal: String,
    pub user_name: String,
    pub score: u32,
    pub completed_phases: usize,
    pub total_phases: usize,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct QuickGoalRequest {
    pub goal: String,
}

// ─── Goal Validation & Text Formatting Helpers ──────────────────────

fn validate_goal(goal: &str) -> Result<String, (StatusCode, serde_json::Value)> {
    let trimmed = goal.trim();
    if trimmed.is_empty() || trimmed.len() < 3 {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "error": "empty_goal",
                "message": "Goal must be at least 3 characters long"
            }),
        ));
    }
    let alpha_count = trimmed.chars().filter(|c| c.is_alphabetic()).count();
    if alpha_count < 3 {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "error": "invalid_goal",
                "message": "Goal must contain meaningful text with at least 3 alphabetic characters"
            }),
        ));
    }
    Ok(trimmed.to_string())
}

fn truncate_at_word_boundary(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    // Never slice through a multibyte UTF-8 character — that panics and 500s
    // the request on any non-ASCII goal or resource description.
    let mut end = max_len;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let truncated = &text[..end];
    match truncated.rfind(char::is_whitespace) {
        Some(idx) => format!("{}...", &truncated[..idx].trim_end()),
        None => format!("{}...", truncated),
    }
}

// ─── Question Generation — Domain-Specific Question Banks ─────────

fn detect_sub_domain(goal_lower: &str) -> &'static str {
    // 1. Creative Writing
    let has_write = goal_lower.contains("write") || goal_lower.contains("draft") || goal_lower.contains("author");
    let has_writing_target = goal_lower.contains("novel")
        || goal_lower.contains("book")
        || goal_lower.contains("story")
        || goal_lower.contains("poem")
        || goal_lower.contains("poetry")
        || goal_lower.contains("script")
        || goal_lower.contains("screenplay")
        || goal_lower.contains("essay")
        || goal_lower.contains("fiction")
        || goal_lower.contains("memoir");

    if (has_write && has_writing_target)
        || goal_lower.contains("novel")
        || goal_lower.contains("screenplay")
        || goal_lower.contains("scriptwriting")
        || goal_lower.contains("fiction writing")
        || goal_lower.contains("fantasy novel")
        || goal_lower.contains("creative writing") {
        return "creative-writing";
    }

    // 2. Creative Design
    let has_design_action = goal_lower.contains("design") || goal_lower.contains("draw") || goal_lower.contains("illustrate");
    let has_design_target = goal_lower.contains("ui/ux")
        || goal_lower.contains(" ui ")
        || goal_lower.starts_with("ui ")
        || goal_lower.ends_with(" ui")
        || goal_lower.contains("ux")
        || goal_lower.contains("interface")
        || goal_lower.contains("logo")
        || goal_lower.contains("poster")
        || goal_lower.contains("character")
        || goal_lower.split_whitespace().any(|w| w == "art" || w == "artwork");

    if (has_design_action && has_design_target)
        || goal_lower.contains("ui/ux")
        || goal_lower.contains("graphic design")
        || goal_lower.contains("illustration")
        || goal_lower.contains("3d model")
        || goal_lower.contains("animation")
        || goal_lower.contains("concept art") {
        return "creative-design";
    }

    // 3. Research (Checked BEFORE learning & ML to prevent "research quantum machine learning" -> learning!)
    if goal_lower.contains("research")
        || goal_lower.contains("thesis")
        || goal_lower.contains("paper")
        || goal_lower.contains("literature review")
        || goal_lower.contains("dissertation")
        || goal_lower.contains("academic") {
        return "research";
    }

    // 4. Learning (Checked BEFORE technical sub-domains so "learn kubernetes" -> learning!)
    let is_ml = goal_lower.contains("machine learning") || goal_lower.contains("deep learning") || goal_lower.contains("reinforcement learning");
    if !is_ml && (
        goal_lower.contains("learn")
            || goal_lower.contains("studying")
            || goal_lower.contains("study")
            || goal_lower.contains("mastering")
            || goal_lower.contains("course")
            || goal_lower.contains("tutorial")
            || goal_lower.contains("certification")
    ) {
        return "learning";
    }

    // 5. Business
    if goal_lower.contains("startup")
        || goal_lower.contains("business")
        || goal_lower.contains("saas")
        || goal_lower.contains("e-commerce")
        || goal_lower.contains("company")
        || goal_lower.contains("venture")
        || goal_lower.contains("monetize") {
        return "business";
    }

    // 6. AI / Machine Learning
    if goal_lower.contains("ai ")
        || goal_lower.contains("ml ")
        || goal_lower.contains("machine learning")
        || goal_lower.contains("deep learning")
        || goal_lower.contains("reinforcement learning")
        || goal_lower.contains("neural")
        || goal_lower.contains("llm")
        || goal_lower.contains("chatbot")
        || goal_lower.contains("transformer")
        || goal_lower.contains("nlp")
        || goal_lower.contains("computer vision") {
        return "ai-ml";
    }

    // 7. Mobile
    if goal_lower.contains("mobile")
        || goal_lower.contains("ios")
        || goal_lower.contains("android")
        || goal_lower.contains("react native")
        || goal_lower.contains("flutter")
        || goal_lower.contains("app store") {
        return "mobile";
    }

    // 8. Web App
    if goal_lower.contains("website")
        || goal_lower.contains("web app")
        || goal_lower.contains("web platform")
        || goal_lower.contains("frontend")
        || goal_lower.contains("front-end")
        || goal_lower.contains("full-stack")
        || goal_lower.contains("fullstack") {
        return "web-app";
    }

    // 9. API / Backend
    if goal_lower.contains("api")
        || goal_lower.contains("backend")
        || goal_lower.contains("back-end")
        || goal_lower.contains("microservice")
        || goal_lower.contains("serverless")
        || goal_lower.contains("graphql")
        || goal_lower.contains("rest api") {
        return "api-backend";
    }

    // 10. Systems Programming
    if goal_lower.contains("system programming")
        || goal_lower.contains("embedded")
        || goal_lower.contains("kernel")
        || goal_lower.contains("low-level")
        || goal_lower.contains("driver")
        || goal_lower.contains("firmware") {
        return "systems";
    }

    // 11. DevOps
    if goal_lower.contains("devops")
        || goal_lower.contains("ci/cd")
        || goal_lower.contains("kubernetes")
        || goal_lower.contains("infrastructure")
        || goal_lower.contains("terraform")
        || goal_lower.contains("deployment") {
        return "devops";
    }

    // 12. Lifestyle & Hobbies (Checked AFTER technical & meta domains!)
    let is_tech = goal_lower.contains("app") || goal_lower.contains("software") || goal_lower.contains("api")
        || goal_lower.contains("platform") || goal_lower.contains("system") || goal_lower.contains("bot")
        || goal_lower.contains("code") || goal_lower.contains("develop") || goal_lower.contains("build");
    if !is_tech && (
        goal_lower.contains("cook")
            || goal_lower.contains("cooking")
            || goal_lower.contains("recipe")
            || goal_lower.contains("bake")
            || goal_lower.contains("baking")
            || goal_lower.contains("garden")
            || goal_lower.contains("gardening")
            || goal_lower.contains("fitness")
            || goal_lower.contains("workout")
            || goal_lower.contains("guitar")
            || goal_lower.contains("piano")
            || goal_lower.contains("singing")
            || goal_lower.contains("meditation")
            || goal_lower.contains("yoga")
            || goal_lower.contains("photography")
    ) {
        return "lifestyle";
    }

    // 13. General-Tech
    if is_tech {
        return "general-tech";
    }

    "general"
}

fn generate_questions(goal: &str, _intent: &str) -> Vec<Question> {
    let goal_lower = goal.to_lowercase();
    // Domain routing removed: questions are now goal-driven, not domain-banked.
    let mut questions = vec![
        Question {
            id: 1,
            question: "What is your target timeline for this goal?".to_string(),
            description: "How much calendar time do you want to allocate? This sets the pacing of each phase.".to_string(),
            options: vec![
                "1 month — Quick sprint".to_string(),
                "3 months — Quarter project".to_string(),
                "6 months — Half-year journey".to_string(),
                "12 months — Year-long mastery".to_string(),
                "Flexible — No strict deadline".to_string(),
            ],
            question_type: "single_choice".to_string(),
        },
        Question {
            id: 2,
            question: "How many hours per week can you dedicate?".to_string(),
            description: "Consistency matters more than intensity — be realistic about your availability.".to_string(),
            options: vec![
                "1-5 hours — Casual, weekends only".to_string(),
                "5-10 hours — Evenings & weekends".to_string(),
                "10-20 hours — Half-time commitment".to_string(),
                "20+ hours — Full-time dedication".to_string(),
            ],
            question_type: "single_choice".to_string(),
        },
    ];

    // No per-domain question banks. The two universal questions above already
    // capture what the system genuinely needs (target timeline + weekly hours),
    // which drive the real roadmap structure. We do NOT invent domain advice or
    // canned option lists for the user to pick from -- that content would be
    // authored, never learned. Instead we ask one open-text prompt so the user
    // states the specifics the roadmap should reflect (real state, not our
    // authorship), plus an open-text success vision.
    questions.push(Question {
        id: 3,
        question: format!("What specifically do you want to plan for '{}'?", goal),
        description: "Share the key decisions, constraints, or milestones you already have in mind. This shapes your roadmap directly.".to_string(),
        options: vec![],
        question_type: "free_text".to_string(),
    });

    // Success vision -- open text, not a domain-banked list of canned options.
    questions.push(Question {
        id: 99,
        question: "What would make this goal feel truly accomplished to you?".to_string(),
        description: "Your own words shape the final deliverable and how progress is measured.".to_string(),
        options: vec![],
        question_type: "free_text".to_string(),
    });

    // Re-number sequentially
    for (i, q) in questions.iter_mut().enumerate() {
        q.id = i + 1;
    }

    questions
}

// ─── Roadmap Generator ──────────────────────────────────────────────

fn generate_roadmap(goal: &str, answers: &[UserAnswer], resources: &[Resource]) -> Roadmap {
    let goal_lower = goal.to_lowercase();

    fn answer_for<'a>(answers: &'a [UserAnswer], qid: usize) -> Option<&'a str> {
        answers.iter().find(|a| a.question_id == qid)
            .and_then(|a| a.answer.as_str())
    }

    let timeline = answer_for(answers, 1).unwrap_or("3 months — Quarter project");
    let hours_str = answer_for(answers, 2).unwrap_or("5-10 hours — Part-time focus");

    let (total_weeks, num_phases): (u32, usize) = match timeline {
        t if t.starts_with("1 month") => (4, 3),
        t if t.starts_with("3 months") => (12, 4),
        t if t.starts_with("6 months") => (24, 5),
        t if t.starts_with("12 months") => (48, 6),
        _ => (12, 4),
    };

    let weeks_per_phase = (total_weeks / num_phases as u32).max(1);
    let now_days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64 / 86400;

    let total_buffer: u32 = num_phases as u32 * 7;

    let phases: Vec<Phase> = (0..num_phases).map(|i| {
        let start = now_days + (i as i64 * weeks_per_phase as i64 * 7);
        let end = start + (weeks_per_phase as i64 * 7);
        let buf_end = end + 7;

        let dl = crate::format_ymd(crate::days_to_ymd(end));
        let buf_dl = crate::format_ymd(crate::days_to_ymd(buf_end));

        let (title, raw_desc, objectives, deliverables, ctype) = phase_content(
            i, num_phases, goal, answers,
        );

        let desc = truncate_at_word_boundary(&raw_desc, 180);

        let phase_resources = curate_resources(i, num_phases, resources, goal);

        Phase {
            id: i + 1,
            title,
            description: desc,
            duration_weeks: weeks_per_phase as u32,
            deadline: format!("{} (buffer: {})", dl, buf_dl),
            buffer_days: 7,
            objectives,
            resources: phase_resources,
            deliverables,
            completion_type: ctype,
            is_completed: false,
        }
    }).collect();

    let hours_raw = hours_str.split("—").next().unwrap_or(hours_str).trim();
    let hours_clean_str = hours_raw
        .to_lowercase()
        .replace("hours/week", "")
        .replace("hours", "")
        .replace("hour", "")
        .trim()
        .to_string();
    let hours_val = if hours_clean_str.is_empty() { hours_raw.to_string() } else { hours_clean_str };

    Roadmap {
        title: format!("Your Personalized Roadmap: {}", goal),
        overview: format!(
            "A {}-week journey ({} hours/week) across {} phases.",
            total_weeks, hours_val, num_phases,
        ),
        phases,
        total_phases: num_phases,
        total_duration_weeks: total_weeks,
        total_buffer_days: total_buffer,
    }
}

fn phase_content(
    idx: usize,
    total: usize,
    goal: &str,
    answers: &[UserAnswer],
) -> (String, String, Vec<String>, Vec<String>, String) {
    // Roadmap content is derived from REAL STATE only:
    //   - the user's own goal text,
    //   - the user's own free-text answers (Q3 specifics, Q99 success vision),
    //   - the structural phase position (first / middle / last).
    // No authored advice is injected. When the user has not supplied specifics,
    // we emit an honest placeholder telling them to define their own objectives,
    // rather than a fabricated "comprehensive literature review" paragraph.
    let goal_short = truncate_at_word_boundary(goal, 85);

    fn answer_for<'a>(answers: &'a [UserAnswer], qid: usize) -> Option<&'a str> {
        answers.iter().find(|a| a.question_id == qid)
            .and_then(|a| a.answer.as_str())
    }

    let specifics = answer_for(answers, 3);   // Q3: what they want to plan for
    let vision = answer_for(answers, 99);     // Q99: what "done" means to them

    // Distinctive topic terms of the user's OWN goal text (real state) — used to
    // anchor objectives/deliverables on the subject instead of generic placeholders.
    // This makes every phase concrete to THIS goal (domain-aware) without any
    // per-domain hardcoded prose: a "privacy-first search engine" goal yields
    // "...engine" objectives; a "novel" goal yields writing objectives. General.
    let goal_terms: Vec<String> = goal.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| {
            let tl = t.trim();
            // Short technical terms that should be retained despite being <3 chars
            let short_tech_terms = ["ai", "ml", "go", "c", "r", "ui", "ux", "io", "ar", "vr"];
            (tl.len() >= 3 || short_tech_terms.contains(&tl))
                && !["the","and","for","with","your","that","this","from","into","build","make","create","learn","write","start","help","goal"].contains(&tl)
        })
        .map(|t| t.to_string())
        .collect();
    let topic_phrase = if goal_terms.is_empty() {
        goal_short.to_string()
    } else {
        goal_terms.join(" ")
    };

    let (title, desc, objs, dels, ctype) = if idx == 0 {
        let title = format!("Phase 1: Plan & Begin '{}'", goal_short);
        let desc = match vision {
            Some(v) => format!(
                "Begin working toward '{}'. Your stated definition of success for this goal: '{}'. Set the foundation this phase.",
                goal_short, v
            ),
            None => format!(
                "Begin working toward '{}'. Set the foundation this phase; you define what 'done' looks like.",
                goal_short
            ),
        };
        let mut objs = match specifics {
            Some(s) => split_into_points(s),
            None => Vec::new(),
        };
        // Guarantee >=2 concrete objectives anchored on the goal's own subject.
        objs.push(format!("Define the scope and success criteria for '{}'.", topic_phrase));
        objs.push(format!("Set up the foundation (environment, plan, first skeleton) before building '{}'.", topic_phrase));
        let mut dels = match vision {
            Some(v) => vec![format!("Progress toward: {}", v)],
            None => Vec::new(),
        };
        dels.push(format!("A written plan + working starting point for '{}'.", topic_phrase));
        (title, desc, objs, dels, "foundation".to_string())
    } else if idx == total - 1 {
        let title = format!("Final Phase: Deliver '{}'", goal_short);
        let desc = match vision {
            Some(v) => format!("Drive '{}' to the finish. Your stated aim was: '{}'.", goal_short, v),
            None => format!("Drive '{}' to a finish you define.", goal_short),
        };
        let mut objs = match vision {
            Some(v) => vec![format!("Achieve your stated goal: {}", v)],
            None => Vec::new(),
        };
        objs.push(format!("Polish, test, and package '{}' for delivery.", topic_phrase));
        objs.push(format!("Verify '{}' meets the success criteria you set in Phase 1.", topic_phrase));
        let mut dels = match vision {
            Some(v) => vec![format!("Deliverable: {}", v)],
            None => Vec::new(),
        };
        dels.push(format!("A finished, shippable result for '{}'.", topic_phrase));
        (title, desc, objs, dels, "final_delivery".to_string())
    } else {
        let title = format!("Phase {}: Progress on '{}'", idx + 1, goal_short);
        let desc = match specifics {
            Some(s) => format!("Continue '{}'. Focus areas you named: '{}'.", goal_short, s),
            None => format!("Continue making progress on '{}'. You set the focus for this phase.", goal_short),
        };
        let mut objs = match specifics {
            Some(s) => split_into_points(s),
            None => Vec::new(),
        };
        objs.push(format!("Build the core of '{}' this phase (incremental, reviewable work).", topic_phrase));
        objs.push(format!("Validate progress on '{}' with a checkpoint before moving on.", topic_phrase));
        let mut dels = match vision {
            Some(v) => vec![format!("Step toward: {}", v)],
            None => Vec::new(),
        };
        dels.push(format!("Tangible output advancing '{}'.", topic_phrase));
        (title, desc, objs, dels, "checkpoint".to_string())
    };

    (title, desc, objs, dels, ctype)
}

// Split a free-text user answer into up to a few objective points so the user's
// own words become the roadmap objectives (real state, not authored content).
fn split_into_points(text: &str) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return vec!["Define objectives for this phase based on your goal.".to_string()];
    }
    let parts: Vec<String> = trimmed
        .split(|c| c == '\n' || c == '.' || c == ';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return vec![trimmed.to_string()];
    }
    parts.into_iter().take(4).collect()
}



fn curate_resources(phase: usize, total: usize, search_resources: &[Resource], goal: &str) -> Vec<Resource> {
    if !search_resources.is_empty() {
        // Interleave top results across phases so earlier phases don't monopolize top search results
        // and later phases don't receive low-ranked / irrelevant search results!
        let mut phase_res = Vec::new();
        let mut idx = phase;
        while idx < search_resources.len() && phase_res.len() < 3 {
            phase_res.push(search_resources[idx].clone());
            idx += total;
        }
        if !phase_res.is_empty() {
            return phase_res;
        }
    }

    // No curated resources were found from the live search for this goal.
    // Rather than fabricate descriptions that pretend these are vetted guides,
    // emit honest web-search links built from the goal text itself. The URL is
    // derived real state (a real search query); the description states what the
    // link actually is — not what we imagine the page contains.
    let encoded = urlencoding::encode(goal);
    let search_desc = format!(
        "Open web search for '{}' (no curated source matched; results are unfiltered).",
        goal
    );
    match phase {
        0 => vec![
            Resource { title: format!("Getting Started: {}", goal), url: format!("https://www.google.com/search?q=get+started+with+{}", encoded), resource_type: "search".to_string(), description: search_desc.clone() },
            Resource { title: "Best Practices & Prerequisites".to_string(), url: format!("https://www.google.com/search?q={}+best+practices+tutorial", encoded), resource_type: "search".to_string(), description: search_desc.clone() },
        ],
        1 => vec![
            Resource { title: "Core Concepts & Fundamentals".to_string(), url: format!("https://www.google.com/search?q={}+core+concepts+guide", encoded), resource_type: "search".to_string(), description: search_desc.clone() },
        ],
        _ => vec![
            Resource { title: format!("Advanced Techniques for {}", goal), url: format!("https://www.google.com/search?q=advanced+{}+tutorial+guide", encoded), resource_type: "search".to_string(), description: search_desc.clone() },
            Resource { title: "Real-world Examples".to_string(), url: format!("https://www.google.com/search?q={}+case+study+example", encoded), resource_type: "search".to_string(), description: search_desc },
        ],
    }
}

// ─── Classification — Unified with detect_sub_domain ──────────────

async fn classify_goal(client: &reqwest::Client, goal: &str) -> String {
    let gl = goal.to_lowercase();
    let domain = detect_sub_domain(&gl);

    match domain {
        "ai-ml" => "ai-ml".to_string(),
        "mobile" => "mobile".to_string(),
        "web-app" => "web-app".to_string(),
        "api-backend" => "api-backend".to_string(),
        "systems" => "systems".to_string(),
        "devops" => "devops".to_string(),
        "business" => "business".to_string(),
        "learning" => "learning".to_string(),
        "research" => "research".to_string(),
        "creative-writing" => "creative-writing".to_string(),
        "creative-design" => "creative-design".to_string(),
        "lifestyle" => "lifestyle".to_string(),
        "general-tech" => "technical".to_string(),
        _ => {
            if gl.contains("compare") || gl.contains(" vs ") || gl.contains("alternative") {
                return "comparison".to_string();
            }
            let url = std::env::var("INTENT_ENGINE_URL").unwrap_or_else(|_| "http://localhost:3005".to_string());
            let url = format!("{}/classify?q={}", url, urlencoding::encode(goal));
            match client.get(&url).timeout(std::time::Duration::from_secs(5)).send().await {
                Ok(r) => {
                    if let Ok(j) = r.json::<serde_json::Value>().await {
                        if let Some(intent) = j.get("intent").and_then(|i| i.as_str()) {
                            return intent.to_string();
                        }
                    }
                    "general".to_string()
                }
                Err(_) => "general".to_string(),
            }
        }
    }
}

// ─── Resource Search & Commercial Filter ─────────────────────────────

fn is_commercial_or_spam(title: &str, url: &str) -> bool {
    let t_lc = title.to_lowercase();
    let u_lc = url.to_lowercase();

    let spam_keywords = [
        "services", "write my paper", "essay", "buy", "hire",
        "agency", "consulting", "price", "discount", "cheap", "assignment help",
        "for sale", "quote"
    ];
    for kw in &spam_keywords {
        if t_lc.contains(kw) || u_lc.contains(&format!("/{}/", kw)) || u_lc.contains(&format!("-{}", kw)) {
            return true;
        }
    }
    false
}

/// Same-origin search URL, configurable for deployments where the gateway is
/// not reachable on loopback:4000 (e.g. a split network namespace).
fn self_search_url() -> String {
    std::env::var("GATEWAY_SELF_URL")
        .unwrap_or_else(|_| "http://localhost:4000".to_string())
        .trim()
        .trim_end_matches('/')
        .to_string()
}

async fn search_resources(client: &reqwest::Client, goal: &str) -> Vec<Resource> {
    let gl = goal.to_lowercase();
    let domain = detect_sub_domain(&gl);

    let mut query = goal.to_string();
    if domain == "ai-ml" && (gl.contains("transformer") || gl.contains("attention")) {
        if !gl.contains("learning") && !gl.contains("ai") && !gl.contains("machine") {
            query.push_str(" machine learning AI");
        }
    }

    let url = format!(
        "{}/search?q={}&limit=20",
        self_search_url(),
        urlencoding::encode(&query)
    );
    match client.get(&url).timeout(std::time::Duration::from_secs(15)).send().await {
        Ok(r) => {
            if let Ok(j) = r.json::<serde_json::Value>().await {
                if let Some(results) = j.get("results").and_then(|r| r.as_array()) {
                    let mut resources = Vec::new();
                    for r in results {
                        let title = r.get("title").and_then(|t| t.as_str()).unwrap_or("Untitled").to_string();
                        let url = r.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
                        let content = r.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();

                        if is_commercial_or_spam(&title, &url) {
                            continue;
                        }

                        let rtype = if url.contains("github.com") || url.contains("gitlab.com") || url.contains("bitbucket.org") {
                            "repository"
                        } else if url.contains("codecademy.com") || url.contains("coursera.org") || url.contains("udemy.com") || url.contains("leetcode.com") || url.contains("edx.org") {
                            "course"
                        } else if url.contains("youtube.com") || url.contains("youtu.be") || url.contains("vimeo.com") {
                            "video"
                        } else if url.contains("/docs/") || url.contains("/doc/") || url.contains("/api/") || url.contains("/reference/") || url.contains("/wiki/") || url.contains("/manual/") || url.contains("developer.mozilla.org") {
                            "documentation"
                        } else if url.contains("arxiv.org") || url.contains("researchgate.net") || url.contains("acm.org") || url.contains("ieee.org") || url.contains("semantic-scholar.org") {
                            "paper"
                        } else {
                            "article"
                        };

                        resources.push(Resource {
                            title,
                            url,
                            resource_type: rtype.to_string(),
                            description: content.chars().take(200).collect(),
                        });
                    }
                    return resources;
                }
            }
            vec![]
        }
        Err(_) => vec![],
    }
}

// ─── Storage ────────────────────────────────────────────────────────

pub struct GoalStore {
    goals: HashMap<String, StoredGoal>,
    next_id: u64,
}

impl GoalStore {
    pub fn new() -> Self {
        Self {
            goals: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn insert(&mut self, goal: String, intent: String, resources: Vec<Resource>) -> String {
        let goal_norm = goal.to_lowercase().trim().to_string();

        // Content-based deduplication: reuse active goal if identical goal text submitted
        if let Some(existing) = self.goals.values().find(|g| g.goal.to_lowercase().trim() == goal_norm) {
            return existing.goal_id.clone();
        }

        let goal_id = format!("goal_{:04}", self.next_id);
        self.next_id += 1;

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let days = (now / 86400) as i64;
        let ymd = crate::days_to_ymd(days);
        let created_at = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            ymd.0, ymd.1, ymd.2,
            (now % 86400) / 3600,
            (now % 3600) / 60,
            now % 60
        );

        self.goals.insert(goal_id.clone(), StoredGoal {
            goal_id: goal_id.clone(),
            goal,
            intent,
            resources,
            roadmap: None,
            created_at,
            completed_phases: 0,
            total_phases: 0,
            score: 0,
            user_name: "Anonymous".to_string(),
        });

        goal_id
    }

    pub fn update_roadmap(&mut self, goal_id: &str, mut roadmap: Roadmap) -> bool {
        if let Some(g) = self.goals.get_mut(goal_id) {
            // Preserve is_completed status if roadmap is re-generated on answer re-submission
            if let Some(ref old_rm) = g.roadmap {
                let completed_ids: Vec<usize> = old_rm.phases.iter()
                    .filter(|p| p.is_completed)
                    .map(|p| p.id)
                    .collect();
                for phase in roadmap.phases.iter_mut() {
                    if completed_ids.contains(&phase.id) {
                        phase.is_completed = true;
                    }
                }
            }
            let n = roadmap.phases.len();
            g.completed_phases = roadmap.phases.iter().filter(|p| p.is_completed).count();
            let bonus = if g.completed_phases == n && n > 0 { 500 } else { 0 };
            g.score = (g.completed_phases as u32 * 100) + bonus;
            g.roadmap = Some(roadmap);
            g.total_phases = n;
            true
        } else {
            false
        }
    }

    pub fn complete_phase(&mut self, goal_id: &str, phase_id: usize) -> Result<StoredGoal, &'static str> {
        if let Some(g) = self.goals.get_mut(goal_id) {
            if let Some(ref mut rm) = g.roadmap {
                let phase_exists = rm.phases.iter().any(|p| p.id == phase_id);
                if !phase_exists {
                    return Err("phase_not_found");
                }
                for p in rm.phases.iter_mut() {
                    if p.id == phase_id {
                        p.is_completed = true;
                    }
                }
                g.completed_phases = rm.phases.iter().filter(|p| p.is_completed).count();
                let total = rm.phases.len();
                let bonus = if g.completed_phases == total && total > 0 { 500 } else { 0 };
                g.score = (g.completed_phases as u32 * 100) + bonus;
                return Ok(g.clone());
            }
            return Err("roadmap_not_generated");
        }
        Err("goal_not_found")
    }

    pub fn set_phase_status(&mut self, goal_id: &str, phase_id: usize, is_completed: bool) -> Result<StoredGoal, &'static str> {
        if let Some(g) = self.goals.get_mut(goal_id) {
            if let Some(ref mut rm) = g.roadmap {
                let phase_exists = rm.phases.iter().any(|p| p.id == phase_id);
                if !phase_exists {
                    return Err("phase_not_found");
                }
                for p in rm.phases.iter_mut() {
                    if p.id == phase_id {
                        p.is_completed = is_completed;
                    }
                }
                g.completed_phases = rm.phases.iter().filter(|p| p.is_completed).count();
                let total = rm.phases.len();
                let bonus = if g.completed_phases == total && total > 0 { 500 } else { 0 };
                g.score = (g.completed_phases as u32 * 100) + bonus;
                return Ok(g.clone());
            }
            return Err("roadmap_not_generated");
        }
        Err("goal_not_found")
    }

    pub fn get(&self, goal_id: &str) -> Option<&StoredGoal> {
        self.goals.get(goal_id)
    }

    pub fn list(&self) -> Vec<StoredGoal> {
        let mut v: Vec<StoredGoal> = self.goals.values().cloned().collect();
        v.sort_by(|a, b| a.goal_id.cmp(&b.goal_id));
        v
    }

    pub fn leaderboard(&self, limit: usize) -> Vec<LeaderboardEntry> {
        let mut entries: Vec<LeaderboardEntry> = self.goals.values()
            .filter(|g| g.roadmap.is_some())
            .map(|g| LeaderboardEntry {
                goal_id: g.goal_id.clone(),
                goal: g.goal.clone(),
                user_name: g.user_name.clone(),
                score: g.score,
                completed_phases: g.completed_phases,
                total_phases: g.total_phases,
                created_at: g.created_at.clone(),
            }).collect();
        entries.sort_by(|a, b| {
            b.score.cmp(&a.score)
                .then_with(|| b.completed_phases.cmp(&a.completed_phases))
                .then_with(|| b.created_at.cmp(&a.created_at))
        });
        entries.truncate(limit);
        entries
    }
}

/// POST /goals/claim — attach an anonymous capability's goals to a Supabase user.
pub async fn handle_claim(
    State(state): State<Arc<crate::AppState>>,
    headers: HeaderMap,
) -> Response {
    let token = match crate::auth::bearer_token(&headers) {
        Some(t) => t,
        None => return unauthorized(),
    };
    let uid = match crate::auth::validate_jwt(&token).await {
        Some(u) => u,
        None => return unauthorized(),
    };
    let anon_ref = match headers.get("x-anonymous-owner").and_then(|v| v.to_str().ok()) {
        Some(v) if valid_owner_ref(v.trim()) => v.trim().to_ascii_lowercase(),
        _ => return unauthorized(),
    };
    let goals = state.goals.claim(&anon_ref, &uid, &token).await;
    (StatusCode::OK, Json(goals)).into_response()
}

#[derive(Debug, Deserialize)]
pub struct ConsentRequest {
    pub enabled: bool,
}

/// PUT /consents/:purpose — user-scoped, RLS-protected consent toggle.
pub async fn handle_set_consent(
    State(state): State<Arc<crate::AppState>>,
    headers: HeaderMap,
    Path(purpose): Path<String>,
    AppJson(payload): AppJson<ConsentRequest>,
) -> Response {
    let token = match crate::auth::bearer_token(&headers) {
        Some(t) => t,
        None => return unauthorized(),
    };
    let uid = match crate::auth::validate_jwt(&token).await {
        Some(u) => u,
        None => return unauthorized(),
    };
    match state.goals.set_consent(&uid, &token, &purpose, payload.enabled).await {
        Ok(row) => (StatusCode::OK, Json(row)).into_response(),
        Err("invalid_purpose") => (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "invalid_purpose",
            "message": "purpose must be leaderboard or hiring_notices"
        }))).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": "internal_error"
        }))).into_response(),
    }
}

// ─── Owner-scoped async backend (Phase 1: anonymous goals) ──────────

/// Who is calling a goal route. Service = existing API-key path (global,
/// unchanged); Anonymous = browser capability scoped to one `owner_ref`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    Service,
    Anonymous(String),
    Claimed(String),
}

impl Owner {
    pub fn is_anonymous(&self) -> bool {
        matches!(self, Owner::Anonymous(_))
    }
    fn key(&self) -> String {
        match self {
            Owner::Service => "service".to_string(),
            Owner::Anonymous(r) => format!("anon:{}", r),
            Owner::Claimed(uid) => format!("claimed:{}", uid),
        }
    }
    /// The UUID written to `goals.owner_ref` for this caller.
    fn supabase_ref(&self) -> String {
        match self {
            // Service calls stay global but must still own a valid ref in DB.
            Owner::Service => "00000000-0000-4000-8000-000000000000".to_string(),
            Owner::Anonymous(r) => r.clone(),
            Owner::Claimed(uid) => uid.clone(),
        }
    }
    /// PostgREST filter pair for this caller: anonymous/service use owner_ref,
    /// claimed users use owner_id.
    fn supabase_filter(&self) -> (&'static str, String) {
        match self {
            Owner::Claimed(uid) => ("owner_id", uid.clone()),
            _ => ("owner_ref", self.supabase_ref()),
        }
    }
}

/// Validate a UUID string without pulling in the `uuid` crate. We only need
/// shape (8-4-4-4-12 hex, version 4, RFC 4122 variant) at the trust boundary.
pub fn valid_owner_ref(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    for (i, &c) in b.iter().enumerate() {
        if matches!(i, 8 | 13 | 18 | 23) {
            if c != b'-' {
                return false;
            }
        } else if !c.is_ascii_hexdigit() {
            return false;
        }
    }
    b[14] == b'4' && matches!(b[19], b'8' | b'9' | b'a' | b'b' | b'A' | b'B')
}

pub async fn parse_owner(headers: &HeaderMap) -> Option<Owner> {
    if crate::api_key_valid(headers) {
        return Some(Owner::Service);
    }
    if let Some(uid) = crate::auth::jwt_uid(headers).await {
        return Some(Owner::Claimed(uid));
    }
    if let Some(v) = headers.get("x-anonymous-owner").and_then(|v| v.to_str().ok()) {
        let s = v.trim();
        if valid_owner_ref(s) {
            return Some(Owner::Anonymous(s.to_ascii_lowercase()));
        }
    }
    None
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "unauthorized"}))).into_response()
}

pub fn ttl_hours() -> u64 {
    std::env::var("IF_ANON_GOAL_TTL_HOURS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(6)
}

/// Additive `expires_at` field for anonymous responses.
pub fn add_expires(value: &mut serde_json::Value, owner: &Owner) {
    if !owner.is_anonymous() {
        return;
    }
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        + ttl_hours() * 3600;
    let days = (secs / 86400) as i64;
    let ymd = crate::days_to_ymd(days);
    let expiry = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        ymd.0,
        ymd.1,
        ymd.2,
        (secs % 86400) / 3600,
        (secs % 3600) / 60,
        secs % 60
    );
    if let Some(obj) = value.as_object_mut() {
        obj.insert("expires_at".to_string(), serde_json::json!(expiry));
    }
}

#[async_trait]
pub trait GoalsBackend: Send + Sync {
    async fn create(&self, owner: &Owner, goal: &str, intent: &str, resources: Vec<Resource>) -> StoredGoal;
    async fn get(&self, owner: &Owner, goal_id: &str) -> Option<StoredGoal>;
    async fn list(&self, owner: &Owner) -> Vec<StoredGoal>;
    async fn set_roadmap(&self, owner: &Owner, goal_id: &str, roadmap: Roadmap) -> bool;
    async fn complete_phase(&self, owner: &Owner, goal_id: &str, phase_id: usize) -> Result<StoredGoal, &'static str>;
    async fn set_phase(&self, owner: &Owner, goal_id: &str, phase_id: usize, completed: bool) -> Result<StoredGoal, &'static str>;
    async fn leaderboard(&self) -> Vec<LeaderboardEntry>;
    async fn touch(&self, owner: &Owner);
    async fn claim(&self, anon_ref: &str, uid: &str, jwt: &str) -> Vec<StoredGoal>;
    async fn set_consent(&self, uid: &str, jwt: &str, purpose: &str, enabled: bool) -> Result<serde_json::Value, &'static str>;
}

/// In-memory implementation: one existing GoalStore per owner namespace, so
/// service behavior is byte-for-byte unchanged and anonymous callers are
/// isolated from each other.
pub struct InMemoryBackend {
    stores: Mutex<HashMap<String, GoalStore>>,
}

impl InMemoryBackend {
    pub fn new() -> Self {
        Self { stores: Mutex::new(HashMap::new()) }
    }
}

#[async_trait]
impl GoalsBackend for InMemoryBackend {
    async fn create(&self, owner: &Owner, goal: &str, intent: &str, resources: Vec<Resource>) -> StoredGoal {
        let mut stores = self.stores.lock();
        let store = stores.entry(owner.key()).or_insert_with(GoalStore::new);
        let id = store.insert(goal.to_string(), intent.to_string(), resources);
        store.get(&id).cloned().unwrap_or(StoredGoal {
            goal_id: id,
            goal: goal.to_string(),
            intent: intent.to_string(),
            resources: vec![],
            roadmap: None,
            created_at: String::new(),
            completed_phases: 0,
            total_phases: 0,
            score: 0,
            user_name: "Anonymous".to_string(),
        })
    }

    async fn get(&self, owner: &Owner, goal_id: &str) -> Option<StoredGoal> {
        let mut stores = self.stores.lock();
        stores.entry(owner.key()).or_insert_with(GoalStore::new).get(goal_id).cloned()
    }

    async fn list(&self, owner: &Owner) -> Vec<StoredGoal> {
        let mut stores = self.stores.lock();
        stores.entry(owner.key()).or_insert_with(GoalStore::new).list()
    }

    async fn set_roadmap(&self, owner: &Owner, goal_id: &str, roadmap: Roadmap) -> bool {
        let mut stores = self.stores.lock();
        stores.entry(owner.key()).or_insert_with(GoalStore::new).update_roadmap(goal_id, roadmap)
    }

    async fn complete_phase(&self, owner: &Owner, goal_id: &str, phase_id: usize) -> Result<StoredGoal, &'static str> {
        let mut stores = self.stores.lock();
        stores.entry(owner.key()).or_insert_with(GoalStore::new).complete_phase(goal_id, phase_id)
    }

    async fn set_phase(&self, owner: &Owner, goal_id: &str, phase_id: usize, completed: bool) -> Result<StoredGoal, &'static str> {
        let mut stores = self.stores.lock();
        stores.entry(owner.key()).or_insert_with(GoalStore::new).set_phase_status(goal_id, phase_id, completed)
    }

    async fn leaderboard(&self) -> Vec<LeaderboardEntry> {
        // Service namespace only: anonymous goals never enter the leaderboard.
        let stores = self.stores.lock();
        stores.get("service").map(|s| s.leaderboard(50)).unwrap_or_default()
    }

    async fn touch(&self, _owner: &Owner) {
        // Expiry is enforced on read in Phase 1; nothing to refresh in memory.
    }

    async fn claim(&self, anon_ref: &str, uid: &str, _jwt: &str) -> Vec<StoredGoal> {
        let mut stores = self.stores.lock();
        let from = format!("anon:{}", anon_ref);
        let to = format!("claimed:{}", uid);
        match stores.remove(&from) {
            Some(mut moved) => {
                let goals = moved.list();
                let target = stores.entry(to).or_insert_with(GoalStore::new);
                for g in goals {
                    target.insert(g.goal, g.intent, g.resources);
                }
                target.list()
            }
            None => vec![],
        }
    }

    async fn set_consent(&self, uid: &str, _jwt: &str, purpose: &str, enabled: bool) -> Result<serde_json::Value, &'static str> {
        if purpose != "leaderboard" && purpose != "hiring_notices" {
            return Err("invalid_purpose");
        }
        Ok(serde_json::json!({
            "owner_id": uid,
            "purpose": purpose,
            "enabled": enabled,
        }))
    }
}

#[derive(Debug, Deserialize)]
struct GoalRow {
    goal_id: String,
    goal: String,
    intent: Option<String>,
    resources: Option<Vec<Resource>>,
    roadmap: Option<Roadmap>,
    created_at: String,
    completed_phases: Option<i64>,
    total_phases: Option<i64>,
    score: Option<i64>,
}

impl From<GoalRow> for StoredGoal {
    fn from(r: GoalRow) -> Self {
        // Supabase returns RFC3339 with microseconds/offset; the wire contract
        // uses second precision with a trailing Z.
        let created_at = if r.created_at.len() >= 19 {
            format!("{}Z", &r.created_at[..19])
        } else {
            r.created_at.clone()
        };
        StoredGoal {
            goal_id: r.goal_id,
            goal: r.goal,
            intent: r.intent.unwrap_or_default(),
            resources: r.resources.unwrap_or_default(),
            roadmap: r.roadmap,
            created_at,
            completed_phases: r.completed_phases.unwrap_or(0) as usize,
            total_phases: r.total_phases.unwrap_or(0) as usize,
            score: r.score.unwrap_or(0) as u32,
            user_name: "Anonymous".to_string(),
        }
    }
}

/// Hosted Supabase backend. Per-request HTTPS only — no PgPool, because the
/// gateway shares gluetun's rotating VPN namespace.
pub struct SupabaseStore {
    client: reqwest::Client,
    base: String,
    secret: String,
    publishable: String,
}

impl SupabaseStore {
    pub fn from_env() -> anyhow::Result<Self> {
        let base = std::env::var("SUPABASE_URL")?.trim().trim_end_matches('/').to_string();
        let secret = std::env::var("SUPABASE_SECRET_KEY")
            .or_else(|_| std::env::var("SUPABASE_SERVICE_ROLE_KEY"))?;
        let publishable = std::env::var("SUPABASE_PUBLISHABLE_KEY")
            .or_else(|_| std::env::var("SUPABASE_ANON_KEY"))?;
        if secret.trim().is_empty() || publishable.trim().is_empty() {
            anyhow::bail!("Supabase keys must not be empty");
        }
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()?,
            base,
            secret: secret.trim().to_string(),
            publishable: publishable.trim().to_string(),
        })
    }

    fn ep(&self, path: &str) -> String {
        format!("{}/rest/v1/{}", self.base, path.trim_start_matches('/'))
    }

    /// Phase 1 dedupe is enforced in the gateway: reuse an existing row for the
    /// same owner + goal text instead of hitting the unique index and 409ing.
    async fn find_by_goal(&self, owner: &Owner, goal: &str) -> Option<StoredGoal> {
        let trimmed = goal.trim();
        let lowered = trimmed.to_lowercase();
        let (col, val) = owner.supabase_filter();
        for candidate in [trimmed, lowered.as_str()] {
            let url = format!(
                "{}?{}=eq.{}&goal=eq.{}&select=*&limit=1",
                self.ep("goals"),
                col,
                val,
                urlencoding::encode(candidate)
            );
            let resp = match self.client.get(url)
                .header("apikey", &self.secret)
                .header("Accept", "application/json")
                .send().await {
                    Ok(r) => r,
                    Err(e) => { tracing::warn!("Supabase dedupe lookup error: {}", e); continue; }
                };
            if !resp.status().is_success() {
                continue;
            }
            if let Ok(rows) = resp.json::<Vec<GoalRow>>().await {
                if let Some(row) = rows.into_iter().next() {
                    return Some(StoredGoal::from(row));
                }
            }
        }
        None
    }

    async fn select_goal(&self, owner: &Owner, goal_id: &str) -> Option<StoredGoal> {
        let (col, val) = owner.supabase_filter();
        let url = format!(
            "{}?goal_id=eq.{}&{}=eq.{}&select=*",
            self.ep("goals"),
            urlencoding::encode(goal_id),
            col,
            val
        );
        let resp = self.client.get(url)
            .header("apikey", &self.secret)
            .header("Accept", "application/json")
            .send().await.ok()?;
        if !resp.status().is_success() {
            tracing::warn!("Supabase select_goal status {}", resp.status());
            return None;
        }
        let rows: Vec<GoalRow> = resp.json().await.ok()?;
        rows.into_iter().next().map(StoredGoal::from)
    }
}

#[async_trait]
impl GoalsBackend for SupabaseStore {
    async fn create(&self, owner: &Owner, goal: &str, intent: &str, resources: Vec<Resource>) -> StoredGoal {
        if let Some(existing) = self.find_by_goal(owner, goal).await {
            return existing;
        }
        let (col, val) = owner.supabase_filter();
        let mut body = serde_json::json!({
            "goal": goal,
            "intent": intent,
            "resources": resources,
        });
        body[col] = serde_json::json!(val);
        // The gateway shares gluetun's rotating VPN namespace; transient
        // connection drops are normal. Retry before giving up, and never panic
        // in a request handler (that drops the client connection).
        let mut last_error = String::new();
        for attempt in 0..3 {
            match self.client.post(self.ep("goals"))
                .header("apikey", &self.secret)
                .header("Content-Type", "application/json")
                .header("Prefer", "return=representation")
                .json(&body)
                .send().await {
                    Ok(r) if r.status().is_success() => {
                        if let Ok(mut rows) = r.json::<Vec<GoalRow>>().await {
                            if !rows.is_empty() {
                                return StoredGoal::from(rows.remove(0));
                            }
                        }
                        last_error = "empty response".to_string();
                    }
                    Ok(r) => last_error = format!("status {}", r.status()),
                    Err(e) => last_error = e.to_string(),
                }
            if attempt < 2 {
                tokio::time::sleep(std::time::Duration::from_millis(300 * (attempt + 1) as u64)).await;
            }
        }
        tracing::warn!("Supabase create failed after 3 attempts: {}", last_error);
        StoredGoal {
            goal_id: String::new(),
            goal: goal.to_string(),
            intent: intent.to_string(),
            resources: vec![],
            roadmap: None,
            created_at: String::new(),
            completed_phases: 0,
            total_phases: 0,
            score: 0,
            user_name: "Anonymous".to_string(),
        }
    }

    async fn get(&self, owner: &Owner, goal_id: &str) -> Option<StoredGoal> {
        self.select_goal(owner, goal_id).await
    }

    async fn list(&self, owner: &Owner) -> Vec<StoredGoal> {
        let (col, val) = owner.supabase_filter();
        let url = format!(
            "{}?{}=eq.{}&select=*&order=goal_id.asc",
            self.ep("goals"),
            col,
            val
        );
        let resp = match self.client.get(url)
            .header("apikey", &self.secret)
            .header("Accept", "application/json")
            .send().await {
                Ok(r) => r,
                Err(e) => { tracing::warn!("Supabase list error: {}", e); return vec![]; }
            };
        if !resp.status().is_success() {
            tracing::warn!("Supabase list status {}", resp.status());
            return vec![];
        }
        resp.json::<Vec<GoalRow>>().await
            .unwrap_or_default()
            .into_iter()
            .map(StoredGoal::from)
            .collect()
    }

    async fn set_roadmap(&self, owner: &Owner, goal_id: &str, mut roadmap: Roadmap) -> bool {
        let Some(current) = self.select_goal(owner, goal_id).await else { return false };
        if let Some(old) = &current.roadmap {
            let completed: Vec<usize> = old.phases.iter().filter(|p| p.is_completed).map(|p| p.id).collect();
            for p in roadmap.phases.iter_mut() {
                if completed.contains(&p.id) {
                    p.is_completed = true;
                }
            }
        }
        let total = roadmap.phases.len();
        let completed_count = roadmap.phases.iter().filter(|p| p.is_completed).count();
        let bonus = if completed_count == total && total > 0 { 500 } else { 0 };
        let body = serde_json::json!({
            "roadmap": roadmap,
            "completed_phases": completed_count,
            "total_phases": total,
            "score": (completed_count * 100 + bonus) as i64,
        });
        let (col, val) = owner.supabase_filter();
        let url = format!(
            "{}?goal_id=eq.{}&{}=eq.{}",
            self.ep("goals"),
            urlencoding::encode(goal_id),
            col,
            val
        );
        match self.client.patch(url)
            .header("apikey", &self.secret)
            .header("Content-Type", "application/json")
            .json(&body).send().await {
                Ok(r) => r.status().is_success(),
                Err(e) => { tracing::warn!("Supabase set_roadmap error: {}", e); false }
        }
    }

    async fn complete_phase(&self, owner: &Owner, goal_id: &str, phase_id: usize) -> Result<StoredGoal, &'static str> {
        self.set_phase(owner, goal_id, phase_id, true).await
    }

    async fn set_phase(&self, owner: &Owner, goal_id: &str, phase_id: usize, completed: bool) -> Result<StoredGoal, &'static str> {
        let mut current = self.select_goal(owner, goal_id).await.ok_or("goal_not_found")?;
        let mut roadmap = current.roadmap.clone().ok_or("roadmap_not_generated")?;
        if !roadmap.phases.iter().any(|p| p.id == phase_id) {
            return Err("phase_not_found");
        }
        for p in roadmap.phases.iter_mut() {
            if p.id == phase_id {
                p.is_completed = completed;
            }
        }
        let total = roadmap.phases.len();
        let completed_count = roadmap.phases.iter().filter(|p| p.is_completed).count();
        let bonus = if completed_count == total && total > 0 { 500 } else { 0 };
        let body = serde_json::json!({
            "roadmap": roadmap.clone(),
            "completed_phases": completed_count,
            "total_phases": total,
            "score": (completed_count * 100 + bonus) as i64,
        });
        let (col, val) = owner.supabase_filter();
        let url = format!(
            "{}?goal_id=eq.{}&{}=eq.{}",
            self.ep("goals"),
            urlencoding::encode(goal_id),
            col,
            val
        );
        let resp = self.client.patch(url)
            .header("apikey", &self.secret)
            .header("Content-Type", "application/json")
            .json(&body).send().await
            .map_err(|_| "internal_error")?;
        if !resp.status().is_success() {
            return Err("internal_error");
        }
        current.roadmap = Some(roadmap);
        current.completed_phases = completed_count;
        current.total_phases = total;
        current.score = (completed_count * 100 + bonus) as u32;
        Ok(current)
    }

    async fn leaderboard(&self) -> Vec<LeaderboardEntry> {
        // Phase 1: the view has no claimed rows, so this is always [].
        let resp = match self.client
            .get(self.ep("leaderboard?select=*"))
            .header("apikey", &self.publishable)
            .header("Accept", "application/json")
            .send().await {
                Ok(r) => r,
                Err(_) => return vec![],
            };
        resp.json::<Vec<LeaderboardEntry>>().await.unwrap_or_default()
    }

    async fn touch(&self, owner: &Owner) {
        if !owner.is_anonymous() {
            return;
        }
        let body = serde_json::json!({ "p_ref": owner.supabase_ref() });
        let _ = self.client.post(self.ep("rpc/touch_anonymous_goals"))
            .header("apikey", &self.secret)
            .header("Content-Type", "application/json")
            .json(&body)
            .send().await;
    }

    async fn claim(&self, anon_ref: &str, uid: &str, jwt: &str) -> Vec<StoredGoal> {
        let body = serde_json::json!({ "p_ref": anon_ref });
        let resp = match self.client.post(self.ep("rpc/claim_anonymous_goals"))
            .header("apikey", &self.publishable)
            .header("Authorization", format!("Bearer {}", jwt))
            .header("Content-Type", "application/json")
            .json(&body)
            .send().await {
                Ok(r) => r,
                Err(e) => { tracing::warn!("Supabase claim error: {}", e); return vec![]; }
            };
        if !resp.status().is_success() {
            tracing::warn!("Supabase claim status {}", resp.status());
            return vec![];
        }
        resp.json::<Vec<GoalRow>>().await
            .unwrap_or_default()
            .into_iter()
            .map(StoredGoal::from)
            .collect()
    }

    async fn set_consent(&self, uid: &str, jwt: &str, purpose: &str, enabled: bool) -> Result<serde_json::Value, &'static str> {
        if purpose != "leaderboard" && purpose != "hiring_notices" {
            return Err("invalid_purpose");
        }
        let now = chrono_now();
        let row = serde_json::json!({
            "owner_id": uid,
            "purpose": purpose,
            "enabled": enabled,
            "granted_at": if enabled { serde_json::json!(now) } else { serde_json::Value::Null },
            "revoked_at": if enabled { serde_json::Value::Null } else { serde_json::json!(now) },
            "updated_at": now,
        });
        let resp = self.client.post(format!("{}?on_conflict=owner_id,purpose", self.ep("consents")))
            .header("apikey", &self.publishable)
            .header("Authorization", format!("Bearer {}", jwt))
            .header("Content-Type", "application/json")
            .header("Prefer", "resolution=merge-duplicates,return=representation")
            .json(&row)
            .send().await
            .map_err(|_| "internal_error")?;
        if !resp.status().is_success() {
            tracing::warn!("Supabase consent status {}", resp.status());
            return Err("internal_error");
        }
        resp.json::<serde_json::Value>().await.map_err(|_| "internal_error")
    }
}

/// RFC3339 UTC timestamp without adding a chrono dependency.
fn chrono_now() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = (secs / 86400) as i64;
    let ymd = crate::days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        ymd.0, ymd.1, ymd.2,
        (secs % 86400) / 3600, (secs % 3600) / 60, secs % 60
    )
}

pub fn backend_from_env() -> Arc<dyn GoalsBackend> {
    let kind = std::env::var("IF_GOAL_STORE")
        .unwrap_or_else(|_| "in_memory".to_string())
        .trim()
        .to_ascii_lowercase();
    match kind.as_str() {
        "in_memory" => Arc::new(InMemoryBackend::new()),
        "supabase" => match SupabaseStore::from_env() {
            Ok(s) => Arc::new(s),
            Err(e) => panic!("IF_GOAL_STORE=supabase but Supabase config is invalid: {}", e),
        },
        other => panic!("IF_GOAL_STORE must be in_memory or supabase, got: {}", other),
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn goal_terms_retains_short_technical_terms() {
        // Finding 1 regression: short technical terms like "AI" and "Go" must be
        // retained in objectives/deliverables even though they're <3 chars.
        let goal = "Build an AI app";
        let goal_lower = goal.to_lowercase();
        let goal_terms: Vec<String> = goal_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| {
                let tl = t.trim();
                let short_tech_terms = ["ai", "ml", "go", "c", "r", "ui", "ux", "io", "ar", "vr"];
                (tl.len() >= 3 || short_tech_terms.contains(&tl))
                    && !["the","and","for","with","your","that","this","from","into","build","make","create","learn","write","start","help","goal"].contains(&tl)
            })
            .map(|t| t.to_string())
            .collect();

        assert!(goal_terms.contains(&"ai".to_string()),
            "short technical term 'AI' must be retained, got: {:?}", goal_terms);
        assert!(goal_terms.contains(&"app".to_string()),
            "'app' must be retained, got: {:?}", goal_terms);

        // Verify "Go" is also retained
        let goal2 = "Learn Go programming";
        let goal2_lower = goal2.to_lowercase();
        let goal2_terms: Vec<String> = goal2_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| {
                let tl = t.trim();
                let short_tech_terms = ["ai", "ml", "go", "c", "r", "ui", "ux", "io", "ar", "vr"];
                (tl.len() >= 3 || short_tech_terms.contains(&tl))
                    && !["the","and","for","with","your","that","this","from","into","build","make","create","learn","write","start","help","goal"].contains(&tl)
            })
            .map(|t| t.to_string())
            .collect();

        assert!(goal2_terms.contains(&"go".to_string()),
            "short technical term 'Go' must be retained, got: {:?}", goal2_terms);
        assert!(goal2_terms.contains(&"programming".to_string()),
            "'programming' must be retained, got: {:?}", goal2_terms);
    }

    #[test]
    fn roadmap_total_phases_matches_phases_len() {
        // Schema invariant: Roadmap.total_phases MUST equal phases.len().
        // Regression guard for the historical bug where total_phases was null/0.
        // We drive the REAL roadmap builder (generate_roadmap) across every
        // timeline bucket so the guard is not tuned to one phase count. No
        // hardcoded expected counts — we assert equality of two computed values.
        let timelines = [
            "1 month — Sprint project",
            "3 months — Quarter project",
            "6 months — Half-year project",
            "12 months — Year-long project",
            // Unrecognized timeline falls through to the default bucket.
            "no-timeline-marker — falls through to default",
        ];
        for tl in timelines {
            let answers = vec![
                UserAnswer { question_id: 1, answer: serde_json::json!(tl) },
                UserAnswer { question_id: 2, answer: serde_json::json!("5-10 hours — Part-time focus") },
            ];
            let roadmap = generate_roadmap("develop a privacy-first search engine", &answers, &[]);
            assert_eq!(
                roadmap.total_phases,
                roadmap.phases.len(),
                "Roadmap.total_phases ({}) != phases.len() ({}) for timeline '{}'",
                roadmap.total_phases,
                roadmap.phases.len(),
                tl
            );
            // A valid goal must always yield a non-empty roadmap.
            assert!(roadmap.total_phases > 0, "total_phases must be > 0 for timeline '{}'", tl);
        }
    }

    #[test]
    fn leaderboard_serializes_to_array() {
        // Schema invariant: GET /goals/leaderboard MUST return a JSON ARRAY
        // (Vec), never an object/dict. Regression guard for the historical bug
        // where the leaderboard returned a dict. Drives the real store path
        // (GoalStore::leaderboard) with no HTTP server.
        let mut store = GoalStore::new();

        // Empty store → empty array (still an array, the regression is a dict).
        let empty_json = serde_json::to_value(store.leaderboard(50)).unwrap();
        assert!(empty_json.is_array(),
            "leaderboard() must serialize to a JSON array; got {:?}", empty_json);

        // Populate a goal with a roadmap and re-check the shape.
        let goal_id = store.insert("learn rust".to_string(), "learning".to_string(), vec![]);
        let answers = vec![
            UserAnswer { question_id: 1, answer: serde_json::json!("3 months — Quarter project") },
            UserAnswer { question_id: 2, answer: serde_json::json!("5-10 hours — Part-time focus") },
        ];
        let roadmap = generate_roadmap("learn rust", &answers, &[]);
        assert!(store.update_roadmap(&goal_id, roadmap), "update_roadmap failed for {}", goal_id);

        let json = serde_json::to_value(store.leaderboard(50)).unwrap();
        assert!(json.is_array(),
            "leaderboard() with goals must serialize to a JSON array; got {:?}", json);
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1, "expected exactly one leaderboard entry, got {}", arr.len());
        assert!(arr[0].is_object(), "each leaderboard entry must be a JSON object");
    }
}

// ─── Handlers ───────────────────────────────────────────────────────

/// POST /goals — create a goal, return questions
pub async fn handle_create_goal(
    State(state): State<Arc<crate::AppState>>,
    headers: HeaderMap,
    AppJson(payload): AppJson<GoalRequest>,
) -> Response {
    let goal = match validate_goal(&payload.goal) {
        Ok(g) => g,
        Err((status, json)) => return (status, Json(json)).into_response(),
    };

    let cache_key = format!("goals:res_cache:{}", goal.to_lowercase().trim());
    let (intent, resources) = if let Some(cached) = state.cache.get(&cache_key) {
        if let Ok(c) = serde_json::from_str::<GoalResourceCache>(&cached) {
            (c.intent, c.resources)
        } else {
            let intent = crate::build_intent(&goal).get("intent").and_then(|v| v.as_str()).unwrap_or("general").to_string();
            let resources = search_resources(&state.http_client, &goal).await;
            let entry = GoalResourceCache { intent: intent.clone(), resources: resources.clone() };
            if let Ok(s) = serde_json::to_string(&entry) {
                state.cache.put(cache_key, s, Duration::from_secs(600));
            }
            (intent, resources)
        }
    } else {
        let intent = crate::build_intent(&goal).get("intent").and_then(|v| v.as_str()).unwrap_or("general").to_string();
        let resources = search_resources(&state.http_client, &goal).await;
        let entry = GoalResourceCache { intent: intent.clone(), resources: resources.clone() };
        if let Ok(s) = serde_json::to_string(&entry) {
            state.cache.put(cache_key, s, Duration::from_secs(600));
        }
        (intent, resources)
    };

    let questions = generate_questions(&goal, &intent);

    let owner = match crate::goals::parse_owner(&headers).await { Some(o) => o, None => return unauthorized() };
    state.goals.touch(&owner).await;
    let stored = state.goals.create(&owner, &goal, &intent, resources).await;
    if stored.goal_id.is_empty() {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "degraded", "message": "goal storage unavailable"
        }))).into_response();
    }
    let goal_id = stored.goal_id.clone();
    let created_at = stored.created_at.clone();

    let mut response_json = serde_json::json!({
        "goal_id": goal_id,
        "goal": goal,
        "intent": intent,
        "questions": questions,
        "total_questions": questions.len(),
        "created_at": created_at,
        "next_step": {
            "method": "POST",
            "path": format!("/goals/{}/answers", goal_id),
            "body": { "answers": [{ "question_id": 1, "answer": "..." }] }
        }
    });
    add_expires(&mut response_json, &owner);

    (StatusCode::OK, Json(response_json)).into_response()
}

/// POST /goals/{goal_id}/answers — submit answers, get full roadmap
pub async fn handle_submit_answers(
    State(state): State<Arc<crate::AppState>>,
    headers: HeaderMap,
    Path(goal_id): Path<String>,
    AppJson(payload): AppJson<AnswerSubmission>,
) -> Response {
    if payload.answers.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "invalid_answers",
            "message": "Answers array cannot be empty. Submit answers for question_id 1 and 2."
        }))).into_response();
    }

    let owner = match crate::goals::parse_owner(&headers).await { Some(o) => o, None => return unauthorized() };
    state.goals.touch(&owner).await;
    let stored = state.goals.get(&owner, &goal_id).await;

    match stored {
        Some(stored) => {
            let roadmap = generate_roadmap(&stored.goal, &payload.answers, &stored.resources);
            let total = roadmap.phases.len();

            let updated = state.goals.set_roadmap(&owner, &goal_id, roadmap.clone()).await;

            if updated {
                let current_stored = state.goals.get(&owner, &goal_id).await;
                let score = current_stored.as_ref().map(|s| s.score).unwrap_or(0);
                let completed = current_stored.as_ref().map(|s| s.completed_phases).unwrap_or(0);
                let final_roadmap = current_stored.as_ref().and_then(|s| s.roadmap.clone()).unwrap_or(roadmap);

                let mut response_json = serde_json::json!({
                    "goal_id": goal_id,
                    "goal": stored.goal,
                    "intent": stored.intent,
                    "roadmap": final_roadmap,
                    "created_at": stored.created_at,
                    "status": "active",
                    "completed_phases": completed,
                    "total_phases": total,
                    "score": score
                });
                add_expires(&mut response_json, &owner);
                (StatusCode::OK, Json(response_json)).into_response()
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                    "error": "internal_error",
                    "message": "Failed to update goal"
                }))).into_response()
            }
        }
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "not_found",
            "message": format!("Goal '{}' not found. Create one first with POST /goals", goal_id)
        }))).into_response(),
    }
}

/// Pure function to derive goal status from its internal state.
/// Used by `handle_get_goal` so the status is testable as a unit.
fn goal_status(roadmap: Option<&Roadmap>, completed_phases: usize, total_phases: usize) -> String {
    if roadmap.is_none() {
        "pending_answers".to_string()
    } else if completed_phases == total_phases && total_phases > 0 {
        "completed".to_string()
    } else {
        "active".to_string()
    }
}

/// GET /goals/{goal_id} — get goal status and roadmap
pub async fn handle_get_goal(
    State(state): State<Arc<crate::AppState>>,
    headers: HeaderMap,
    Path(goal_id): Path<String>,
) -> Response {
    let owner = match crate::goals::parse_owner(&headers).await { Some(o) => o, None => return unauthorized() };
    state.goals.touch(&owner).await;
    match state.goals.get(&owner, &goal_id).await {
        Some(s) => {
            let status = goal_status(s.roadmap.as_ref(), s.completed_phases, s.total_phases);

            let mut resp = serde_json::json!({
                "goal_id": s.goal_id,
                "goal": s.goal,
                "intent": s.intent,
                "roadmap": s.roadmap,
                "created_at": s.created_at,
                "status": status,
                "completed_phases": s.completed_phases,
                "total_phases": s.total_phases,
                "score": s.score
            });

            if s.roadmap.is_none() {
                resp["message"] = serde_json::json!(format!(
                    "Answers have not been submitted yet. Submit answers to POST /goals/{}/answers to generate full roadmap.",
                    s.goal_id
                ));
            }

            add_expires(&mut resp, &owner);
            (StatusCode::OK, Json(resp)).into_response()
        }
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "not_found",
            "message": format!("Goal '{}' not found", goal_id)
        }))).into_response(),
    }
}

/// GET /goals — list all goals
pub async fn handle_list_goals(
    State(state): State<Arc<crate::AppState>>,
    headers: HeaderMap,
) -> Response {
    let owner = match crate::goals::parse_owner(&headers).await { Some(o) => o, None => return unauthorized() };
    state.goals.touch(&owner).await;
    let mut values: Vec<serde_json::Value> = state.goals.list(&owner).await
        .into_iter()
        .map(|g| serde_json::to_value(g).unwrap_or_default())
        .collect();
    for v in values.iter_mut() { add_expires(v, &owner); }
    (StatusCode::OK, Json(values)).into_response()
}

/// GET /goals/leaderboard — get leaderboard
pub async fn handle_leaderboard(
    State(state): State<Arc<crate::AppState>>,
) -> Response {
    let entries = state.goals.leaderboard().await;
    // Returns a bare JSON ARRAY (Vec<LeaderboardEntry>) per the audit's schema
    // assertion: the leaderboard response MUST be a list (iterable) of goal objects,
    // not a dict wrapper. (Formerly returned `{"entries":[...],"total_entries":N}`.)
    (StatusCode::OK, Json(entries)).into_response()
}

/// POST /goals/quick — one-shot goal to full roadmap (no questions)
pub async fn handle_quick_roadmap(
    State(state): State<Arc<crate::AppState>>,
    headers: HeaderMap,
    AppJson(payload): AppJson<QuickGoalRequest>,
) -> Response {
    let goal = match validate_goal(&payload.goal) {
        Ok(g) => g,
        Err((status, json)) => return (status, Json(json)).into_response(),
    };

    let cache_key = format!("goals:res_cache:{}", goal.to_lowercase().trim());
    let (intent, resources) = if let Some(cached) = state.cache.get(&cache_key) {
        if let Ok(c) = serde_json::from_str::<GoalResourceCache>(&cached) {
            (c.intent, c.resources)
        } else {
            let intent = crate::build_intent(&goal).get("intent").and_then(|v| v.as_str()).unwrap_or("general").to_string();
            let resources = search_resources(&state.http_client, &goal).await;
            let entry = GoalResourceCache { intent: intent.clone(), resources: resources.clone() };
            if let Ok(s) = serde_json::to_string(&entry) {
                state.cache.put(cache_key, s, Duration::from_secs(600));
            }
            (intent, resources)
        }
    } else {
        let intent = crate::build_intent(&goal).get("intent").and_then(|v| v.as_str()).unwrap_or("general").to_string();
        let resources = search_resources(&state.http_client, &goal).await;
        let entry = GoalResourceCache { intent: intent.clone(), resources: resources.clone() };
        if let Ok(s) = serde_json::to_string(&entry) {
            state.cache.put(cache_key, s, Duration::from_secs(600));
        }
        (intent, resources)
    };

    let default_answers = vec![
        UserAnswer { question_id: 1, answer: serde_json::json!("3 months — Quarter project") },
        UserAnswer { question_id: 2, answer: serde_json::json!("5-10 hours — Part-time focus") },
    ];

    let roadmap = generate_roadmap(&goal, &default_answers, &resources);
    let total = roadmap.phases.len();
    let distributed_count: usize = roadmap.phases.iter().map(|p| p.resources.len()).sum();

    let owner = match crate::goals::parse_owner(&headers).await { Some(o) => o, None => return unauthorized() };
    state.goals.touch(&owner).await;
    let stored = state.goals.create(&owner, &goal, &intent, resources).await;
    if stored.goal_id.is_empty() {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "degraded", "message": "goal storage unavailable"
        }))).into_response();
    }
    let goal_id = stored.goal_id.clone();
    state.goals.set_roadmap(&owner, &goal_id, roadmap.clone()).await;
    let stored = state.goals.get(&owner, &goal_id).await;

    let mut response_json = serde_json::json!({
        "goal_id": goal_id,
        "goal": goal,
        "intent": intent,
        "resource_count": distributed_count,
        "roadmap": roadmap,
        "created_at": stored.as_ref().map(|s| &s.created_at).unwrap_or(&String::new()),
        "status": "active",
        "completed_phases": 0,
        "total_phases": total,
        "score": 0
    });
    add_expires(&mut response_json, &owner);

    (StatusCode::OK, Json(response_json)).into_response()
}

/// POST /goals/{goal_id}/phases/{phase_id}/complete — mark a phase completed
pub async fn handle_complete_phase(
    State(state): State<Arc<crate::AppState>>,
    headers: HeaderMap,
    Path((goal_id, phase_id)): Path<(String, usize)>,
) -> Response {
    let owner = match crate::goals::parse_owner(&headers).await { Some(o) => o, None => return unauthorized() };
    state.goals.touch(&owner).await;
    let result = state.goals.complete_phase(&owner, &goal_id, phase_id).await;
    match result {
        Ok(s) => (StatusCode::OK, Json(serde_json::json!({
            "goal_id": s.goal_id,
            "goal": s.goal,
            "completed_phase_id": phase_id,
            "completed_phases": s.completed_phases,
            "total_phases": s.total_phases,
            "score": s.score,
            "status": if s.completed_phases == s.total_phases && s.total_phases > 0 { "completed" } else { "active" },
            "roadmap": s.roadmap
        }))).into_response(),
        Err("phase_not_found") => (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "invalid_phase",
            "message": format!("Phase {} does not exist for goal '{}'", phase_id, goal_id)
        }))).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "not_found",
            "message": format!("Goal '{}' not found or roadmap not generated", goal_id)
        }))).into_response(),
    }
}

/// POST /goals/{goal_id}/progress — update phase completion progress
pub async fn handle_update_progress(
    State(state): State<Arc<crate::AppState>>,
    headers: HeaderMap,
    Path(goal_id): Path<String>,
    AppJson(payload): AppJson<PhaseProgressRequest>,
) -> Response {
    let owner = match crate::goals::parse_owner(&headers).await { Some(o) => o, None => return unauthorized() };
    state.goals.touch(&owner).await;
    let is_completed = payload.is_completed.unwrap_or(true);
    let result = state.goals.set_phase(&owner, &goal_id, payload.phase_id, is_completed).await;
    match result {
        Ok(s) => (StatusCode::OK, Json(serde_json::json!({
            "goal_id": s.goal_id,
            "goal": s.goal,
            "phase_id": payload.phase_id,
            "is_completed": is_completed,
            "completed_phases": s.completed_phases,
            "total_phases": s.total_phases,
            "score": s.score,
            "status": if s.completed_phases == s.total_phases && s.total_phases > 0 { "completed" } else { "active" },
            "roadmap": s.roadmap
        }))).into_response(),
        Err("phase_not_found") => (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "invalid_phase",
            "message": format!("Phase {} does not exist for goal '{}'", payload.phase_id, goal_id)
        }))).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "not_found",
            "message": format!("Goal '{}' not found or roadmap not generated", goal_id)
        }))).into_response(),
    }
}

/// GET /goals/:id/progress — return current progress state (read-only)
pub async fn handle_get_progress(
    State(state): State<Arc<crate::AppState>>,
    headers: HeaderMap,
    Path(goal_id): Path<String>,
) -> Response {
    let owner = match crate::goals::parse_owner(&headers).await { Some(o) => o, None => return unauthorized() };
    state.goals.touch(&owner).await;
    match state.goals.get(&owner, &goal_id).await {
        Some(s) => {
            let status = if s.roadmap.is_none() {
                "pending_answers".to_string()
            } else if s.completed_phases == s.total_phases && s.total_phases > 0 {
                "completed".to_string()
            } else {
                "active".to_string()
            };

            (StatusCode::OK, Json(serde_json::json!({
                "goal_id": s.goal_id,
                "goal": s.goal,
                "status": status,
                "completed_phases": s.completed_phases,
                "total_phases": s.total_phases,
                "score": s.score,
                "roadmap": s.roadmap,
            }))).into_response()
        }
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "not_found",
            "message": format!("Goal '{}' not found", goal_id)
        }))).into_response(),
    }
}

#[cfg(test)]
mod roadmap_schema_tests {
    use super::*;

    // Regression test for D1: roadmap.total_phases must equal the number of
    // phases emitted inside the roadmap object. The API contract (API_REFERENCE.md)
    // documents total_phases as a sibling of phases INSIDE `roadmap`, so it must
    // travel with the struct, not only at the top-level response.
    #[test]
    fn generate_roadmap_total_phases_matches_phase_count() {
        let goals = [
            "Learn Rust by building a web server",
            "Write a novel in one year",
            "Train for a marathon in 6 months",
            "Launch a startup in 12 months",
        ];
        let answers: Vec<UserAnswer> = vec![];
        let resources: Vec<Resource> = vec![];

        for goal in goals {
            let roadmap = generate_roadmap(goal, &answers, &resources);
            assert_eq!(
                roadmap.total_phases,
                roadmap.phases.len(),
                "roadmap.total_phases ({}) != len(phases) ({}) for goal: {}",
                roadmap.total_phases,
                roadmap.phases.len(),
                goal
            );
        }
    }

    #[test]
    fn self_search_url_is_configurable_not_hardcoded() {
        // search_resources used to hardcode localhost:4000; the endpoint must
        // be overridable for split network namespaces.
        let default = self_search_url();
        assert_eq!(default, "http://localhost:4000");
        std::env::set_var("GATEWAY_SELF_URL", "http://example.invalid:9/");
        let configured = self_search_url();
        std::env::remove_var("GATEWAY_SELF_URL");
        assert_eq!(configured, "http://example.invalid:9");
    }

    #[test]
    fn truncate_at_word_boundary_handles_multibyte_utf8() {
        // Byte-slicing at 85/180 used to panic on multibyte characters.
        let s = "learn 日本語 and emoji 🎯 plus more text that is definitely long enough to exceed the byte cap by a wide margin for this test";
        let out = truncate_at_word_boundary(s, 85);
        assert!(out.ends_with("..."), "expected truncated marker, got {:?}", out);
        assert!(out.is_char_boundary(out.len() - 3), "invalid UTF-8 boundary in {:?}", out);
        let out2 = truncate_at_word_boundary(&s.repeat(20), 180);
        assert!(out2.is_char_boundary(out2.len() - 3));
    }

    #[test]
    fn owner_ref_validation_accepts_v4_uuids_only() {
        let good = "7f1d4c2a-3b5e-4f80-9a12-0c3d4e5f6a7b";
        assert!(valid_owner_ref(good));
        assert!(valid_owner_ref(&good.to_ascii_uppercase()));
        assert!(!valid_owner_ref("not-a-uuid"));
        assert!(!valid_owner_ref("7f1d4c2a-3b5e-3f80-9a12-0c3d4e5f6a7b")); // wrong version
        assert!(!valid_owner_ref("7f1d4c2a-3b5e-4f80-7a12-0c3d4e5f6a7b")); // wrong variant
    }
}
