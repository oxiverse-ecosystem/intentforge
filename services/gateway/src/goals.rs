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
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

use serde::{Deserialize, Serialize};
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
    pub total_duration_weeks: u32,
    pub total_buffer_days: u32,
}

#[derive(Debug, Clone)]
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
    let truncated = &text[..max_len];
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
    let domain = detect_sub_domain(&goal_lower);

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

    match domain {
        "ai-ml" => {
            questions.push(Question {
                id: 3,
                question: "What type of AI/ML system are you building?".to_string(),
                description: "Different AI systems need different architectures — from simple API wrappers to custom model training pipelines.".to_string(),
                options: vec![
                    "LLM-powered app using existing APIs (OpenAI, Claude, etc.)".to_string(),
                    "Custom model training and fine-tuning".to_string(),
                    "Recommendation / prediction engine".to_string(),
                    "Multi-agent system with tool use".to_string(),
                    "Computer vision or media processing pipeline".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 4,
                question: "What's your data strategy?".to_string(),
                description: "AI/ML projects are data-driven — how will you source, store, and process your data?".to_string(),
                options: vec![
                    "Using existing public datasets".to_string(),
                    "Generating synthetic data".to_string(),
                    "Collecting user-generated data".to_string(),
                    "Streaming real-time data pipeline".to_string(),
                    "Need to acquire/annotate custom data".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 5,
                question: "What compute infrastructure do you need?".to_string(),
                description: "AI workloads vary from CPU-only inference to GPU-intensive training clusters.".to_string(),
                options: vec![
                    "CPU-only — using hosted API services".to_string(),
                    "Single GPU — fine-tuning or small models".to_string(),
                    "Multi-GPU — distributed training".to_string(),
                    "Edge / on-device inference".to_string(),
                    "Cloud TPU or specialized hardware".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 6,
                question: "How will you handle model evaluation and monitoring?".to_string(),
                description: "Production AI needs observability — accuracy tracking, drift detection, and feedback loops.".to_string(),
                options: vec![
                    "Manual evaluation — test on sample cases".to_string(),
                    "Automated benchmark suite".to_string(),
                    "A/B testing in production".to_string(),
                    "Full MLOps pipeline with monitoring".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
        }

        "web-app" | "general-tech" | "api-backend" => {
            questions.push(Question {
                id: 3,
                question: "What architecture pattern do you want to follow?".to_string(),
                description: "The architecture shapes how your components communicate and scale.".to_string(),
                options: vec![
                    "Monolithic — simple, single deployable".to_string(),
                    "Microservices — independent, deployable services".to_string(),
                    "Serverless — functions as a service (AWS Lambda, etc.)".to_string(),
                    "Event-driven — message queues and async processing".to_string(),
                    "Jamstack — static frontend + APIs".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 4,
                question: "How will you handle data persistence and storage?".to_string(),
                description: "Data storage choices affect performance, scalability, and cost.".to_string(),
                options: vec![
                    "Relational database (PostgreSQL, MySQL)".to_string(),
                    "NoSQL document store (MongoDB, Firestore)".to_string(),
                    "Hybrid — SQL + cache layer (Redis)".to_string(),
                    "File/object storage (S3, GCS)".to_string(),
                    "None — fully API-driven, no persistent storage".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 5,
                question: "What's your deployment and hosting strategy?".to_string(),
                description: "Where and how will your application run in production?".to_string(),
                options: vec![
                    "PaaS (Heroku, Railway, Render)".to_string(),
                    "Cloud VM / VPS (AWS EC2, DigitalOcean)".to_string(),
                    "Container / Kubernetes (Docker, EKS, GKE)".to_string(),
                    "Serverless (Lambda, Cloud Functions)".to_string(),
                    "Edge / CDN (Cloudflare Workers, Vercel Edge)".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 6,
                question: "Do you need real-time features?".to_string(),
                description: "Real-time capabilities (chat, live updates, notifications) require specific infrastructure choices.".to_string(),
                options: vec![
                    "No real-time needed — standard request-response".to_string(),
                    "WebSockets for live bidirectional communication".to_string(),
                    "Server-Sent Events for push updates".to_string(),
                    "Polling or periodic background sync".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
        }

        "mobile" => {
            questions.push(Question {
                id: 3,
                question: "What platform(s) are you targeting?".to_string(),
                description: "Platform choice affects development language, tooling, and deployment.".to_string(),
                options: vec![
                    "iOS native (Swift/SwiftUI)".to_string(),
                    "Android native (Kotlin/Jetpack)".to_string(),
                    "Cross-platform (React Native, Flutter)".to_string(),
                    "Progressive Web App (PWA)".to_string(),
                    "All platforms".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 4,
                question: "What backend / API architecture do you need?".to_string(),
                description: "Mobile apps typically need a backend for data, auth, and push notifications.".to_string(),
                options: vec![
                    "No backend — fully offline/local-first".to_string(),
                    "BaaS (Firebase, Supabase)".to_string(),
                    "Custom REST/GraphQL API".to_string(),
                    "Backend + real-time sync (WebSockets)".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 5,
                question: "What's your offline and sync strategy?".to_string(),
                description: "Mobile apps face connectivity challenges — how will you handle offline usage?".to_string(),
                options: vec![
                    "Always-online — no offline support".to_string(),
                    "Cache recent data for offline reading".to_string(),
                    "Full offline-first with background sync".to_string(),
                    "Local database with conflict resolution (CRDT)".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
        }

        "systems" => {
            questions.push(Question {
                id: 3,
                question: "What's your target platform / hardware?".to_string(),
                description: "Systems programming targets vary from embedded MCUs to kernel modules.".to_string(),
                options: vec![
                    "Linux / POSIX systems".to_string(),
                    "Embedded / microcontroller (ARM, RISC-V)".to_string(),
                    "Windows / Win32".to_string(),
                    "Cross-platform / portable".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 4,
                question: "What's your performance profile?".to_string(),
                description: "Systems-level work requires understanding your performance constraints.".to_string(),
                options: vec![
                    "High throughput — processing large volumes".to_string(),
                    "Low latency — real-time constraints".to_string(),
                    "Memory constrained — embedded/limited RAM".to_string(),
                    "Battery efficient — mobile/portable".to_string(),
                    "Safety critical — fault tolerance required".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
        }

        "devops" => {
            questions.push(Question {
                id: 3,
                question: "What's your infrastructure scale?".to_string(),
                description: "The scale determines tooling choices — from simple single-server to multi-cluster orchestration.".to_string(),
                options: vec![
                    "Single server / small project".to_string(),
                    "Multi-service / small cluster".to_string(),
                    "Large-scale / multi-cluster".to_string(),
                    "Edge / multi-region deployment".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 4,
                question: "What's your cloud provider preference?".to_string(),
                description: "Different clouds have different services, pricing, and tooling ecosystems.".to_string(),
                options: vec![
                    "Amazon Web Services (AWS)".to_string(),
                    "Google Cloud Platform (GCP)".to_string(),
                    "Microsoft Azure".to_string(),
                    "Multi-cloud / vendor-neutral".to_string(),
                    "On-premise / self-hosted".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
        }

        "research" => {
            questions.push(Question {
                id: 3,
                question: "What's your research methodology?".to_string(),
                description: "The methodology determines how you'll gather, analyze, and validate your findings.".to_string(),
                options: vec![
                    "Quantitative — experiments, metrics, statistics".to_string(),
                    "Qualitative — interviews, case studies, observations".to_string(),
                    "Mixed methods — both quantitative and qualitative".to_string(),
                    "Literature review / survey paper".to_string(),
                    "Theoretical / mathematical proof".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 4,
                question: "What's your target publication outlet?".to_string(),
                description: "Different venues have different expectations for scope, format, and rigor.".to_string(),
                options: vec![
                    "Conference paper (5-10 pages)".to_string(),
                    "Journal article (10-20 pages)".to_string(),
                    "Pre-print / arXiv".to_string(),
                    "Thesis / dissertation".to_string(),
                    "Blog post / technical report".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 5,
                question: "What tools and resources do you have access to?".to_string(),
                description: "Research tools range from lab equipment to cloud compute and data licenses.".to_string(),
                options: vec![
                    "All public/open resources — no special access".to_string(),
                    "University / institutional resources available".to_string(),
                    "Need to collect/generate my own data".to_string(),
                    "Industry partnership / proprietary data access".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 6,
                question: "Are you collaborating or working solo?".to_string(),
                description: "Team size affects workflow, tooling, and project management approach.".to_string(),
                options: vec![
                    "Solo — just me".to_string(),
                    "Small team — 2-3 collaborators".to_string(),
                    "Research group — 4+ people".to_string(),
                    "Cross-institutional collaboration".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
        }

        "creative-writing" => {
            questions.push(Question {
                id: 3,
                question: "What's your writing genre and format?".to_string(),
                description: "Different genres have different conventions for structure, length, and style.".to_string(),
                options: vec![
                    "Fantasy / Science Fiction — world-building heavy".to_string(),
                    "Literary fiction — character-driven narrative".to_string(),
                    "Non-fiction / memoir — real-world based".to_string(),
                    "Technical writing / documentation".to_string(),
                    "Screenplay / script — dialogue and scene-driven".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 4,
                question: "What's your creative process style?".to_string(),
                description: "Understanding your writing process helps structure the project timeline.".to_string(),
                options: vec![
                    "Plotter — detailed outlines before writing".to_string(),
                    "Pantser — write by the seat of your pants, discover as you go".to_string(),
                    "Plantser — hybrid: loose outline, flexible execution".to_string(),
                    "Iterative — write, revise, restructure multiple passes".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 5,
                question: "What's your editing and revision approach?".to_string(),
                description: "Quality writing requires multiple rounds of revision — how will you handle this?".to_string(),
                options: vec![
                    "Self-edit — multiple personal revision passes".to_string(),
                    "Beta readers — get feedback from trusted readers".to_string(),
                    "Professional editor — hire an editor".to_string(),
                    "Peer review workshop — writing group feedback".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
        }

        "creative-design" => {
            questions.push(Question {
                id: 3,
                question: "What design medium are you working in?".to_string(),
                description: "Different mediums require different tools, techniques, and workflows.".to_string(),
                options: vec![
                    "Digital illustration / 2D art".to_string(),
                    "3D modeling / animation".to_string(),
                    "UI/UX / web design".to_string(),
                    "Graphic design / branding".to_string(),
                    "Motion graphics / video".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 4,
                question: "What's your preferred design toolchain?".to_string(),
                description: "The right tools can dramatically affect your productivity.".to_string(),
                options: vec![
                    "Adobe Creative Suite (Photoshop, Illustrator)".to_string(),
                    "Figma / Sketch (UI/UX focused)".to_string(),
                    "Blender / Maya (3D focused)".to_string(),
                    "Open source tools (GIMP, Inkscape, Krita)".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
        }

        "business" => {
            questions.push(Question {
                id: 3,
                question: "What's your business model?".to_string(),
                description: "The business model drives product decisions, pricing, and revenue strategy.".to_string(),
                options: vec![
                    "SaaS — subscription-based software".to_string(),
                    "Marketplace — connecting buyers and sellers".to_string(),
                    "E-commerce — selling physical/digital products".to_string(),
                    "Freemium — free tier + paid upgrades".to_string(),
                    "Enterprise — sales-led B2B".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 4,
                question: "Who is your target customer?".to_string(),
                description: "Understanding your customer shapes everything from marketing to feature prioritization.".to_string(),
                options: vec![
                    "Individual consumers (B2C)".to_string(),
                    "Small businesses (SMB)".to_string(),
                    "Enterprise companies".to_string(),
                    "Developers / technical audience".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 5,
                question: "What stage is your business at?".to_string(),
                description: "The stage determines priorities — from validation to growth to optimization.".to_string(),
                options: vec![
                    "Idea stage — validating the concept".to_string(),
                    "Pre-launch — building the MVP".to_string(),
                    "Early stage — first customers, iterating".to_string(),
                    "Growth stage — scaling and optimization".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
        }

        "lifestyle" => {
            questions.push(Question {
                id: 3,
                question: "What is your main focus for this activity?".to_string(),
                description: "Focusing your efforts helps tailor the practice schedule and milestones.".to_string(),
                options: vec![
                    "Learning fundamentals and building a routine".to_string(),
                    "Improving technique and personal mastery".to_string(),
                    "Completing a specific personal project or goal".to_string(),
                    "Health, relaxation, and personal enjoyment".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 4,
                question: "How do you prefer to practice or learn?".to_string(),
                description: "Your environment and method shape your daily consistency.".to_string(),
                options: vec![
                    "Self-guided practice with guides/videos".to_string(),
                    "Guided class or instructor-led sessions".to_string(),
                    "Hands-on experimentation and trial".to_string(),
                    "Group practice with friends or community".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
        }

        "learning" => {
            questions.push(Question {
                id: 3,
                question: "What's your learning style preference?".to_string(),
                description: "Different learning styles benefit from different resource types and pacing.".to_string(),
                options: vec![
                    "Structured courses — follow a curriculum".to_string(),
                    "Project-based — learn by building".to_string(),
                    "Documentation-first — read & practice".to_string(),
                    "Video tutorials — watch and code along".to_string(),
                    "Interactive — hands-on labs and exercises".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
            questions.push(Question {
                id: 4,
                question: "What's your assessment goal?".to_string(),
                description: "How will you validate your learning progress?".to_string(),
                options: vec![
                    "Build a portfolio project".to_string(),
                    "Pass a certification exam".to_string(),
                    "Contribute to open source".to_string(),
                    "Apply knowledge at work / real project".to_string(),
                    "Just learning — no specific assessment needed".to_string(),
                ],
                question_type: "single_choice".to_string(),
            });
        }

        _ => {}
    }

    // ── Universal final question (Q99) tailored by domain ────────
    let final_options = match domain {
        "research" => vec![
            "A published paper in a peer-reviewed journal or conference".to_string(),
            "A completed manuscript ready for pre-print submission".to_string(),
            "Validated research findings and experimental datasets".to_string(),
            "A thesis or dissertation ready for defense".to_string(),
            "Personal research milestone and knowledge discovery".to_string(),
        ],
        "creative-writing" => vec![
            "A completed, polished manuscript ready for publishing".to_string(),
            "A formatted book or script submitted to agents/publishers".to_string(),
            "A published work available on target distribution platforms".to_string(),
            "A completed first draft ready for developmental editing".to_string(),
            "Personal creative fulfillment and storytelling milestone".to_string(),
        ],
        "creative-design" => vec![
            "A finished design portfolio piece and case study".to_string(),
            "A complete design system and asset library for production".to_string(),
            "A published visual project or interactive artwork".to_string(),
            "Client-ready design deliverables and prototypes".to_string(),
            "Personal artistic growth and visual design mastery".to_string(),
        ],
        "business" => vec![
            "A launched business with active paying customers and revenue".to_string(),
            "A validated MVP with active beta users and feedback loops".to_string(),
            "An investor-ready pitch deck, business plan, and financial model".to_string(),
            "A scalable product with automated onboarding and growth channels".to_string(),
            "Personal entrepreneurial milestone and venture validation".to_string(),
        ],
        "lifestyle" => vec![
            "Consistently applying and enjoying this skill in daily life".to_string(),
            "Completing a personal project or milestone".to_string(),
            "Sharing my creation or skill with family and friends".to_string(),
            "Improved health, relaxation, and personal fulfillment".to_string(),
            "Personal mastery and lifelong enjoyment".to_string(),
        ],
        "learning" => vec![
            "Deep understanding and practical ability to build real projects".to_string(),
            "Passing an official professional certification exam".to_string(),
            "A completed capstone portfolio project demonstrating mastery".to_string(),
            "Ability to teach, mentor, or lead others in this topic".to_string(),
            "Personal growth and mastering a new skill domain".to_string(),
        ],
        _ => vec![
            "A working prototype I can demo".to_string(),
            "A completed product ready for end users".to_string(),
            "A production-grade system with test coverage and CI/CD".to_string(),
            "A portfolio piece for career opportunities".to_string(),
            "Personal satisfaction and technical mastery".to_string(),
        ],
    };

    questions.push(Question {
        id: 99,
        question: "What would make this goal feel truly accomplished to you?".to_string(),
        description: "Your vision of success helps us shape the final deliverable and measure progress.".to_string(),
        options: final_options,
        question_type: "single_choice".to_string(),
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

    let domain = detect_sub_domain(&goal_lower);
    let is_technical = matches!(domain, "web-app" | "ai-ml" | "mobile" | "systems"
        | "devops" | "api-backend" | "general-tech");

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
            i, num_phases, goal, domain, is_technical,
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
        total_duration_weeks: total_weeks,
        total_buffer_days: total_buffer,
    }
}

fn phase_content(
    idx: usize,
    total: usize,
    goal: &str,
    domain: &str,
    is_technical: bool,
) -> (String, String, Vec<String>, Vec<String>, String) {
    let goal_short = truncate_at_word_boundary(goal, 85);
    let is_ai = domain == "ai-ml";
    let is_research = domain == "research";
    let is_business = domain == "business";
    let is_creative_writing = domain == "creative-writing";
    let is_creative_design = domain == "creative-design";
    let is_learning = domain == "learning";
    let is_lifestyle = domain == "lifestyle";

    if idx == 0 {
        let (title, desc, objs, dels) = if is_research {
            ("Literature Review & Research Design".to_string(),
             format!("Conduct a comprehensive literature review, define your research questions, and design the methodology for '{}'.", goal_short),
             vec!["Conduct systematic literature review".to_string(), "Define research questions and hypotheses".to_string(), "Design methodology and data collection plan".to_string(), "Set up research tools and collaboration workflow".to_string()],
             vec!["Literature review document".to_string(), "Research design / methodology document".to_string(), "Data collection instruments prepared".to_string()])
        } else if is_creative_writing {
            ("Concept Development & Outline".to_string(),
             format!("Develop your narrative concept, outline the core structure, and create world/character guides for '{}'.", goal_short),
             vec!["Develop core concept and premise".to_string(), "Create character bibles and worldbuilding guide".to_string(), "Construct detailed chapter/scene outline".to_string(), "Research genre conventions and style references".to_string()],
             vec!["Story concept and outline document".to_string(), "Character and setting bible".to_string(), "Writing milestones schedule".to_string()])
        } else if is_creative_design {
            ("Concept Development & Research".to_string(),
             format!("Explore design directions, define user flows/moodboards, and establish asset requirements for '{}'.", goal_short),
             vec!["Research design benchmarks and inspirations".to_string(), "Create moodboards and color palettes".to_string(), "Map out user flows / key visual scenes".to_string(), "Set up design workspace and toolchain".to_string()],
             vec!["Design brief and moodboard".to_string(), "User flow diagrams / composition thumbnails".to_string(), "Brand token specifications".to_string()])
        } else if is_learning {
            ("Foundation & Curriculum Planning".to_string(),
             format!("Assess your starting point, gather learning resources, and create a structured study plan for '{}'.", goal_short),
             vec!["Assess current knowledge and skill gaps".to_string(), "Gather high-quality learning resources".to_string(), "Structure study schedule and weekly goals".to_string(), "Set up practice workspace and tooling".to_string()],
             vec!["Skills assessment document".to_string(), "Curated study plan with resources".to_string(), "Development environment ready".to_string()])
        } else if is_business {
            ("Market Research & Strategy".to_string(),
             format!("Validate your business idea, conduct market research, and define value proposition for '{}'.", goal_short),
             vec!["Conduct market research and competitive analysis".to_string(), "Validate problem-solution fit with target users".to_string(), "Define business model and monetization strategy".to_string(), "Create go-to-market plan".to_string()],
             vec!["Market research report".to_string(), "Validated value proposition document".to_string(), "Go-to-market strategy deck".to_string()])
        } else if is_lifestyle || (!is_technical && domain == "general") {
            ("Preparation & Foundational Setup".to_string(),
             format!("Gather materials, study basic principles, and establish a daily routine for '{}'.", goal_short),
             vec!["Gather necessary tools, equipment, or ingredients".to_string(), "Learn core concepts and safety/best practices".to_string(), "Set up dedicated practice space and schedule".to_string()],
             vec!["Preparation checklist complete".to_string(), "Practice schedule established".to_string()])
        } else {
            let title = "Architecture & Planning".to_string();
            let desc = format!("Design system architecture, choose tech stack, and set up foundation for '{}'.", goal_short);
            let objs = if is_ai {
                vec!["Design AI/ML system architecture".to_string(), "Choose model approach and data pipeline".to_string(), "Set up development environment and GPU tooling".to_string(), "Define evaluation metrics and benchmarks".to_string()]
            } else {
                vec!["Design system architecture and component diagram".to_string(), "Choose tech stack and dependencies".to_string(), "Set up development environment and CI/CD".to_string(), "Define API contracts and data models".to_string()]
            };
            let dels = vec!["Architecture document with diagrams".to_string(), "Tech stack decision record".to_string(), "Development environment with CI/CD".to_string()];
            (title, desc, objs, dels)
        };
        return (title, desc, objs, dels, "foundation".to_string());
    }

    if idx == total - 1 {
        let (title, desc, objs, dels) = if is_research {
            ("Publication & Dissemination".to_string(),
             format!("Finalize your paper manuscript, submit to target publication, and present findings for '{}'.", goal_short),
             vec!["Write and format final manuscript".to_string(), "Generate publishable figures and tables".to_string(), "Submit to target journal / conference".to_string(), "Prepare presentation slides and artifact repository".to_string()],
             vec!["Submitted research manuscript".to_string(), "Open data/code repository".to_string(), "Presentation slides".to_string()])
        } else if is_creative_writing {
            ("Production & Publication".to_string(),
             format!("Perform final proofreading, format layout, and publish or submit manuscript for '{}'.", goal_short),
             vec!["Final proofreading and line editing pass".to_string(), "Format book/script for distribution".to_string(), "Prepare cover art and metadata".to_string(), "Execute publishing or submission plan".to_string()],
             vec!["Published / submitted manuscript".to_string(), "Promotional / query package".to_string(), "Distribution confirmation".to_string()])
        } else if is_creative_design {
            ("Final Asset Delivery & Presentation".to_string(),
             format!("Export production assets, compile design portfolio case study, and present '{}'.", goal_short),
             vec!["Export production-ready assets and design specs".to_string(), "Build interactive prototype showcase".to_string(), "Document design system guidelines".to_string(), "Publish case study / portfolio entry".to_string()],
             vec!["Production asset package".to_string(), "Design system documentation".to_string(), "Portfolio case study".to_string()])
        } else if is_learning {
            ("Mastery, Capstone & Portfolio Integration".to_string(),
             format!("Build a capstone project, validate your skills, and integrate learning into your portfolio for '{}'.", goal_short),
             vec!["Complete comprehensive capstone project".to_string(), "Perform self-assessment / certification exam".to_string(), "Publish capstone project on GitHub/Portfolio".to_string(), "Write learning reflection and summary".to_string()],
             vec!["Completed capstone project".to_string(), "Portfolio entry with live demo/repo".to_string(), "Certification / skill validation record".to_string()])
        } else if is_business {
            ("Launch & Growth".to_string(),
             format!("Launch product, acquire first paying customers, and establish growth loops for '{}'.", goal_short),
             vec!["Public product launch and marketing campaign".to_string(), "Onboard first paying customers".to_string(), "Set up analytics and funnel tracking".to_string(), "Establish customer support and feedback channels".to_string()],
             vec!["Live launched product".to_string(), "First customer milestone achieved".to_string(), "Analytics & revenue dashboard".to_string()])
        } else if is_lifestyle || (!is_technical && domain == "general") {
            ("Mastery & Showcase".to_string(),
             format!("Demonstrate your skills, complete a showcase project, and establish a long-term routine for '{}'.", goal_short),
             vec!["Execute a final showcase project / demonstration".to_string(), "Reflect on progress and key learnings".to_string(), "Establish ongoing practice habit".to_string()],
             vec!["Completed showcase project".to_string(), "Skill mastery reflection".to_string()])
        } else {
            ("Launch & Polish".to_string(),
             format!("Complete testing, fix bugs, deploy to production, and create documentation for '{}'.", goal_short),
             vec!["Comprehensive testing and bug fixing".to_string(), "Performance optimization and profiling".to_string(), "Production deployment and domain setup".to_string(), "Documentation and README".to_string()],
             vec!["Deployed production project".to_string(), "System documentation".to_string(), "Demo video / presentation".to_string()])
        };
        let ctype = if is_technical { "project".to_string() } else { "final_delivery".to_string() };
        return (title, desc, objs, dels, ctype);
    }

    // Dynamic Middle Phases (idx from 1 to total-2)
    let mid_count = total - 2;
    let mid_idx = idx - 1;

    let (title, desc, objs, dels, ctype) = if is_lifestyle || (!is_technical && domain == "general") {
        match (mid_count, mid_idx) {
            (1, 0) => (
                "Core Practice & Skill Development".to_string(),
                format!("Build consistency and master core techniques for '{}'.", goal_short),
                vec!["Practice core skills daily/weekly".to_string(), "Learn intermediate techniques".to_string(), "Track progress and overcome hurdles".to_string()],
                vec!["Practice log".to_string(), "Intermediate progress artifact".to_string()],
                "practice_checkpoint".to_string()
            ),
            (2, 0) => (
                "Core Practice & Mechanics".to_string(),
                format!("Develop foundational muscle memory and techniques for '{}'.", goal_short),
                vec!["Execute basic exercises and drills".to_string(), "Build consistency".to_string()],
                vec!["Practice log".to_string()],
                "practice_checkpoint".to_string()
            ),
            (2, 1) => (
                "Technique Refinement & Execution".to_string(),
                format!("Refine techniques and tackle more challenging goals for '{}'.", goal_short),
                vec!["Apply skills to complete tasks".to_string(), "Refine form and efficiency".to_string()],
                vec!["Refinement progress log".to_string()],
                "practice_checkpoint".to_string()
            ),
            _ => (
                format!("Practice Phase {}", mid_idx + 1),
                format!("Progressive skill building for '{}'.", goal_short),
                vec!["Execute practice routine".to_string()],
                vec!["Progress artifact".to_string()],
                "practice_checkpoint".to_string()
            )
        }
    } else if is_learning {
        match (mid_count, mid_idx) {
            (1, 0) => (
                "Core Learning & Hands-On Exercises".to_string(),
                format!("Work through primary course material and build foundational exercises for '{}'.", goal_short),
                vec!["Study core concepts and syntax".to_string(), "Complete guided coding/theory exercises".to_string(), "Build mini-projects to reinforce learning".to_string()],
                vec!["Completed exercise code/notes".to_string(), "Mini-project repository".to_string()],
                "learning_checkpoint".to_string()
            ),
            (2, 0) => (
                "Core Concepts & Guided Practice".to_string(),
                format!("Master core concepts and complete guided tutorials for '{}'.", goal_short),
                vec!["Study fundamental principles".to_string(), "Complete structured tutorials and quizzes".to_string(), "Implement sample exercises".to_string()],
                vec!["Tutorial notes and code samples".to_string(), "Progress log".to_string()],
                "learning_checkpoint".to_string()
            ),
            (2, 1) => (
                "Advanced Topics & Practical Applications".to_string(),
                format!("Deep dive into advanced topics and real-world problem solving for '{}'.", goal_short),
                vec!["Study advanced patterns and edge cases".to_string(), "Solve real-world practice problems".to_string(), "Start building unguided projects".to_string()],
                vec!["Practice project source code".to_string(), "Advanced topic notes".to_string()],
                "learning_checkpoint".to_string()
            ),
            (3, 0) => (
                "Fundamental Principles & Syntax".to_string(),
                format!("Build strong foundational understanding of key concepts in '{}'.", goal_short),
                vec!["Review core documentation and courses".to_string(), "Complete beginner coding labs".to_string(), "Take concept quizzes".to_string()],
                vec!["Study notes and exercise repository".to_string()],
                "learning_checkpoint".to_string()
            ),
            (3, 1) => (
                "Core Mechanics & Guided Building".to_string(),
                format!("Apply core mechanics to construct mini-projects for '{}'.", goal_short),
                vec!["Build 2-3 small standalone projects".to_string(), "Practice debugging and problem solving".to_string(), "Participate in code reviews / community".to_string()],
                vec!["Mini-project implementations".to_string(), "Code review notes".to_string()],
                "learning_checkpoint".to_string()
            ),
            (3, 2) => (
                "Advanced Architecture & Best Practices".to_string(),
                format!("Explore advanced architecture and industry best practices for '{}'.", goal_short),
                vec!["Learn design patterns and optimization".to_string(), "Study production-grade codebases".to_string(), "Refactor existing projects".to_string()],
                vec!["Refactored codebase".to_string(), "Best practices guide".to_string()],
                "learning_checkpoint".to_string()
            ),
            _ => (
                format!("Skill Deepening Phase {}", mid_idx + 1),
                format!("Progressive mastery and practical execution for '{}'.", goal_short),
                vec!["Execute scheduled study topics".to_string(), "Solve complex challenges".to_string(), "Document key insights".to_string()],
                vec!["Phase progress report".to_string(), "Code/notes artifact".to_string()],
                "learning_checkpoint".to_string()
            )
        }
    } else if is_business {
        match (mid_count, mid_idx) {
            (1, 0) => (
                "MVP Development & User Validation".to_string(),
                format!("Build core product features and test with early users for '{}'.", goal_short),
                vec!["Develop minimum viable product (MVP)".to_string(), "Set up user authentication and basic flow".to_string(), "Conduct user testing with 10+ target users".to_string()],
                vec!["Working MVP build".to_string(), "User feedback summary".to_string()],
                "prototype".to_string()
            ),
            (2, 0) => (
                "MVP Core Feature Development".to_string(),
                format!("Build the primary product capabilities and essential workflows for '{}'.", goal_short),
                vec!["Implement core user value proposition features".to_string(), "Build seamless onboarding flow".to_string(), "Integrate analytics tracking".to_string()],
                vec!["Core product prototype".to_string(), "Onboarding flow complete".to_string()],
                "prototype".to_string()
            ),
            (2, 1) => (
                "Beta Testing, Feedback & Monetization Setup".to_string(),
                format!("Run closed beta, iterate based on feedback, and set up billing for '{}'.", goal_short),
                vec!["Run closed beta program with select users".to_string(), "Integrate payment processing (Stripe/Paddle)".to_string(), "Iterate on user UX pain points".to_string()],
                vec!["Beta testing feedback analysis".to_string(), "Integrated payment gateway".to_string()],
                "feature_complete".to_string()
            ),
            (3, 0) => (
                "Core MVP Development".to_string(),
                format!("Construct the foundational product architecture and core features for '{}'.", goal_short),
                vec!["Build core backend APIs and user interface".to_string(), "Set up database and data persistence".to_string(), "Implement core business logic".to_string()],
                vec!["Functional MVP core".to_string(), "API & DB schema document".to_string()],
                "prototype".to_string()
            ),
            (3, 1) => (
                "User Onboarding & Closed Beta".to_string(),
                format!("Onboard early adopters and refine product mechanics for '{}'.", goal_short),
                vec!["Launch closed beta for target cohort".to_string(), "Gather qualitative and quantitative analytics".to_string(), "Refine core UX based on feedback".to_string()],
                vec!["Beta user feedback report".to_string(), "UX improvement list executed".to_string()],
                "prototype".to_string()
            ),
            (3, 2) => (
                "Monetization & Infrastructure Scaling".to_string(),
                format!("Implement revenue mechanics and prepare scalable operations for '{}'.", goal_short),
                vec!["Build subscription/checkout billing system".to_string(), "Implement transactional emails and support".to_string(), "Harden security and cloud infrastructure".to_string()],
                vec!["Billing & payout system ready".to_string(), "Support workflow configured".to_string()],
                "feature_complete".to_string()
            ),
            _ => (
                format!("Business Execution Phase {}", mid_idx + 1),
                format!("Develop, test, and iterate on product/business operations for '{}'.", goal_short),
                vec!["Execute planned business milestones".to_string(), "Gather customer data".to_string(), "Optimize key metrics".to_string()],
                vec!["Milestone deliverable".to_string(), "Metrics log".to_string()],
                "feature_complete".to_string()
            )
        }
    } else if is_research {
        match (mid_count, mid_idx) {
            (1, 0) => (
                "Data Collection & Analysis".to_string(),
                format!("Execute research methodology, collect empirical data, and analyze results for '{}'.", goal_short),
                vec!["Execute data collection / experimentation plan".to_string(), "Perform statistical analysis and hypothesis testing".to_string(), "Document empirical findings".to_string()],
                vec!["Research dataset".to_string(), "Analysis notebooks and code".to_string()],
                "research_milestone".to_string()
            ),
            (2, 0) => (
                "Data Collection & Experimentation".to_string(),
                format!("Gather data and run experimental trials for '{}'.", goal_short),
                vec!["Set up experimental environment".to_string(), "Collect raw data and benchmarks".to_string(), "Clean and structure research dataset".to_string()],
                vec!["Raw dataset".to_string(), "Experimental execution log".to_string()],
                "research_milestone".to_string()
            ),
            (2, 1) => (
                "Data Analysis & Manuscript Drafting".to_string(),
                format!("Analyze data, produce visualizations, and draft research manuscript for '{}'.", goal_short),
                vec!["Perform statistical tests and evaluation".to_string(), "Generate publication-grade figures".to_string(), "Draft Methods and Results sections".to_string()],
                vec!["Analysis figures & tables".to_string(), "Draft manuscript sections".to_string()],
                "research_milestone".to_string()
            ),
            _ => (
                format!("Research Progress Phase {}", mid_idx + 1),
                format!("Execute research plan and synthesize findings for '{}'.", goal_short),
                vec!["Run experimental pipeline".to_string(), "Analyze results".to_string(), "Draft paper sections".to_string()],
                vec!["Research output data".to_string(), "Draft section".to_string()],
                "research_milestone".to_string()
            )
        }
    } else if is_creative_writing {
        match (mid_count, mid_idx) {
            (1, 0) => (
                "Drafting & Manuscript Creation".to_string(),
                format!("Write the primary draft and develop core story arcs for '{}'.", goal_short),
                vec!["Write first draft of chapters / scenes".to_string(), "Maintain character consistency and tone".to_string(), "Complete core manuscript text".to_string()],
                vec!["First manuscript draft".to_string(), "Progress word count log".to_string()],
                "draft".to_string()
            ),
            (2, 0) => (
                "First Draft Writing".to_string(),
                format!("Focus on raw content creation and scene drafting for '{}'.", goal_short),
                vec!["Write daily word count target".to_string(), "Complete major plot points".to_string(), "Finish rough first draft".to_string()],
                vec!["Rough manuscript draft".to_string()],
                "draft".to_string()
            ),
            (2, 1) => (
                "Revision, Editing & Refinement".to_string(),
                format!("Revise structure, edit prose, and polish manuscript for '{}'.", goal_short),
                vec!["Perform self-edit pass for story flow".to_string(), "Gather beta reader feedback".to_string(), "Line edit for tone and clarity".to_string()],
                vec!["Revised manuscript".to_string(), "Editorial notes log".to_string()],
                "draft".to_string()
            ),
            _ => (
                format!("Writing Phase {}", mid_idx + 1),
                format!("Write and edit manuscript content for '{}'.", goal_short),
                vec!["Draft assigned sections".to_string(), "Review and revise text".to_string()],
                vec!["Drafted manuscript section".to_string()],
                "draft".to_string()
            )
        }
    } else if is_creative_design {
        match (mid_count, mid_idx) {
            (1, 0) => (
                "Prototyping & Asset Creation".to_string(),
                format!("Develop visual assets, components, and interactive prototypes for '{}'.", goal_short),
                vec!["Build core UI components / 3D models".to_string(), "Create visual asset library".to_string(), "Assemble interactive prototype".to_string()],
                vec!["Component library".to_string(), "Interactive prototype".to_string()],
                "design_prototype".to_string()
            ),
            (2, 0) => (
                "Wireframing & Visual Exploration".to_string(),
                format!("Create low-fidelity wireframes and visual design options for '{}'.", goal_short),
                vec!["Create low-fidelity screen layouts / sketches".to_string(), "Define typography and color palette".to_string(), "Review visual directions".to_string()],
                vec!["Wireframe layouts".to_string(), "Style guide draft".to_string()],
                "design_prototype".to_string()
            ),
            (2, 1) => (
                "High-Fidelity Design & Polish".to_string(),
                format!("Refine visual fidelity, micro-interactions, and design specs for '{}'.", goal_short),
                vec!["Construct high-fidelity component designs".to_string(), "Add animations and micro-interactions".to_string(), "Conduct usability testing on prototype".to_string()],
                vec!["High-fidelity prototype".to_string(), "Usability test summary".to_string()],
                "design_prototype".to_string()
            ),
            _ => (
                format!("Design Phase {}", mid_idx + 1),
                format!("Create and refine visual designs for '{}'.", goal_short),
                vec!["Design visual components".to_string(), "Integrate user feedback".to_string()],
                vec!["Design assets".to_string()],
                "design_prototype".to_string()
            )
        }
    } else if is_ai {
        match (mid_count, mid_idx) {
            (1, 0) => (
                "Model Development & Pipeline Integration".to_string(),
                format!("Implement data pipeline, train/fine-tune models, and build evaluation suite for '{}'.", goal_short),
                vec!["Set up dataset preprocessing and augmentations".to_string(), "Train baseline and fine-tuned models".to_string(), "Evaluate model performance against benchmarks".to_string()],
                vec!["Trained model weights".to_string(), "Evaluation metric report".to_string()],
                "model_checkpoint".to_string()
            ),
            (2, 0) => (
                "Data Engineering & Baseline Training".to_string(),
                format!("Build data pipeline and train initial baseline models for '{}'.", goal_short),
                vec!["Build clean data extraction & preprocessing pipeline".to_string(), "Implement baseline model pipeline".to_string(), "Log training metrics".to_string()],
                vec!["Data pipeline code".to_string(), "Baseline model evaluation".to_string()],
                "model_checkpoint".to_string()
            ),
            (2, 1) => (
                "Model Optimization & System Integration".to_string(),
                format!("Fine-tune model hyperparameters, optimize inference latency, and integrate with backend for '{}'.", goal_short),
                vec!["Optimize hyperparameters and prompt engineering".to_string(), "Quantize / optimize model inference".to_string(), "Integrate model serving API with main app".to_string()],
                vec!["Optimized model artifact".to_string(), "Inference API endpoint".to_string()],
                "model_checkpoint".to_string()
            ),
            (3, 0) => (
                "Data Pipeline & Feature Engineering".to_string(),
                format!("Construct robust data ingestion and feature processing for '{}'.", goal_short),
                vec!["Build automated data pipeline".to_string(), "Engineered feature set / embeddings".to_string(), "Set up experiment tracking (MLflow/W&B)".to_string()],
                vec!["Data pipeline repository".to_string(), "Feature store setup".to_string()],
                "model_checkpoint".to_string()
            ),
            (3, 1) => (
                "Model Training & Architecture Exploration".to_string(),
                format!("Train multiple model candidates and optimize performance for '{}'.", goal_short),
                vec!["Train candidate architectures".to_string(), "Perform hyperparameter sweeps".to_string(), "Analyze error cases and edge failures".to_string()],
                vec!["Trained model candidates".to_string(), "Experiment leaderboard".to_string()],
                "model_checkpoint".to_string()
            ),
            (3, 2) => (
                "Inference Engine & MLOps Integration".to_string(),
                format!("Build low-latency inference engine and monitoring for '{}'.", goal_short),
                vec!["Build optimized inference server".to_string(), "Add model monitoring and drift alerts".to_string(), "Integrate end-to-end with frontend".to_string()],
                vec!["Inference service build".to_string(), "MLOps dashboard".to_string()],
                "model_checkpoint".to_string()
            ),
            _ => (
                format!("AI Model Phase {}", mid_idx + 1),
                format!("Train and optimize AI components for '{}'.", goal_short),
                vec!["Train model iterations".to_string(), "Evaluate performance".to_string()],
                vec!["Model artifact".to_string()],
                "model_checkpoint".to_string()
            )
        }
    } else {
        // Technical / General-Tech / Web-App / API-Backend / DevOps / Systems
        match (mid_count, mid_idx) {
            (1, 0) => (
                "Core Implementation & Building".to_string(),
                format!("Build core features, components, and integrations for '{}'.", goal_short),
                vec!["Implement core business logic and services".to_string(), "Build main application UI / API routes".to_string(), "Integrate database models and external APIs".to_string(), "Write unit tests for core modules".to_string()],
                vec!["Working prototype with core features".to_string(), "API endpoints complete".to_string(), "Unit test suite".to_string()],
                "prototype".to_string()
            ),
            (2, 0) => (
                "Core Feature Implementation".to_string(),
                format!("Implement primary functionality and core application modules for '{}'.", goal_short),
                vec!["Build foundational modules and data structures".to_string(), "Implement primary API endpoints".to_string(), "Create core user interface views".to_string()],
                vec!["Working core prototype".to_string(), "API documentation draft".to_string()],
                "prototype".to_string()
            ),
            (2, 1) => (
                "System Integration & Comprehensive Testing".to_string(),
                format!("Wire together frontend, backend, and external services for '{}'.", goal_short),
                vec!["Integrate frontend and backend communication".to_string(), "Implement integration and E2E test suite".to_string(), "Optimize queries and network requests".to_string()],
                vec!["Fully wired application".to_string(), "Test coverage report".to_string()],
                "feature_complete".to_string()
            ),
            (3, 0) => (
                "Foundational Core Development".to_string(),
                format!("Build the foundational services and database layer for '{}'.", goal_short),
                vec!["Implement database schema and ORM/query layer".to_string(), "Create core domain logic services".to_string(), "Set up authentication and authorization".to_string()],
                vec!["Data layer and auth build".to_string(), "Core service endpoints".to_string()],
                "prototype".to_string()
            ),
            (3, 1) => (
                "Advanced Feature Expansion".to_string(),
                format!("Build secondary feature set and interactive user flows for '{}'.", goal_short),
                vec!["Implement secondary application features".to_string(), "Add real-time updates / background tasks".to_string(), "Polish user interface components".to_string()],
                vec!["Expanded feature build".to_string(), "UI component library".to_string()],
                "prototype".to_string()
            ),
            (3, 2) => (
                "Integration, Security & Load Testing".to_string(),
                format!("Perform full system integration, security hardening, and performance tuning for '{}'.", goal_short),
                vec!["Conduct end-to-end integration testing".to_string(), "Perform security vulnerability scan and fix issues".to_string(), "Benchmark and tune database queries / response times".to_string()],
                vec!["Security audit clean report".to_string(), "Load test results".to_string()],
                "feature_complete".to_string()
            ),
            (4, 0) => (
                "Core Data & API Architecture".to_string(),
                format!("Build fundamental data structures and API framework for '{}'.", goal_short),
                vec!["Build core data models and database migrations".to_string(), "Construct REST / GraphQL API endpoints".to_string(), "Write unit tests for business logic".to_string()],
                vec!["Data models build".to_string(), "API endpoints".to_string()],
                "prototype".to_string()
            ),
            (4, 1) => (
                "Primary Feature Set Implementation".to_string(),
                format!("Implement primary feature workflows and user interfaces for '{}'.", goal_short),
                vec!["Build main user dashboard and interactive components".to_string(), "Wire frontend components to backend APIs".to_string(), "Implement state management".to_string()],
                vec!["Primary feature suite".to_string(), "Interactive UI build".to_string()],
                "prototype".to_string()
            ),
            (4, 2) => (
                "External Integrations & Real-Time Sync".to_string(),
                format!("Integrate third-party services, webhooks, and real-time messaging for '{}'.", goal_short),
                vec!["Integrate third-party APIs and services".to_string(), "Build WebSocket / async event queues".to_string(), "Implement error resilience and retry logic".to_string()],
                vec!["Integration middleware".to_string(), "Event queue build".to_string()],
                "prototype".to_string()
            ),
            (4, 3) => (
                "Comprehensive Testing & Security Audit".to_string(),
                format!("Execute rigorous end-to-end testing, security audits, and performance tuning for '{}'.", goal_short),
                vec!["Run E2E automated testing suite".to_string(), "Perform security hardening and dependency audit".to_string(), "Optimize memory usage and response latency".to_string()],
                vec!["Automated test suite".to_string(), "Performance and security benchmark report".to_string()],
                "feature_complete".to_string()
            ),
            _ => (
                format!("Technical Development Phase {}", mid_idx + 1),
                format!("Develop and test system components for '{}'.", goal_short),
                vec!["Implement scheduled feature tasks".to_string(), "Perform code quality checks".to_string()],
                vec!["Feature module build".to_string()],
                "prototype".to_string()
            )
        }
    };

    (title, desc, objs, dels, ctype)
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

    let encoded = urlencoding::encode(goal);
    match phase {
        0 => vec![
            Resource { title: format!("Getting Started: {}", goal), url: format!("https://www.google.com/search?q=get+started+with+{}", encoded), resource_type: "article".to_string(), description: "A comprehensive guide to getting started.".to_string() },
            Resource { title: "Best Practices & Prerequisites".to_string(), url: format!("https://www.google.com/search?q={}+best+practices+tutorial", encoded), resource_type: "documentation".to_string(), description: "Learn foundational knowledge.".to_string() },
        ],
        1 => vec![
            Resource { title: "Core Concepts & Fundamentals".to_string(), url: format!("https://www.google.com/search?q={}+core+concepts+guide", encoded), resource_type: "article".to_string(), description: "Deep dive into core concepts.".to_string() },
        ],
        _ => vec![
            Resource { title: format!("Advanced Techniques for {}", goal), url: format!("https://www.google.com/search?q=advanced+{}+tutorial+guide", encoded), resource_type: "article".to_string(), description: "Advanced techniques and best practices.".to_string() },
            Resource { title: "Real-world Examples".to_string(), url: format!("https://www.google.com/search?q={}+case+study+example", encoded), resource_type: "article".to_string(), description: "Learn from real implementations.".to_string() },
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
        "http://localhost:4000/search?q={}&limit=20",
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

// ─── Handlers ───────────────────────────────────────────────────────

/// POST /goals — create a goal, return questions
pub async fn handle_create_goal(
    State(state): State<Arc<crate::AppState>>,
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
            let intent = classify_goal(&state.http_client, &goal).await;
            let resources = search_resources(&state.http_client, &goal).await;
            let entry = GoalResourceCache { intent: intent.clone(), resources: resources.clone() };
            if let Ok(s) = serde_json::to_string(&entry) {
                state.cache.put(cache_key, s, Duration::from_secs(600));
            }
            (intent, resources)
        }
    } else {
        let intent = classify_goal(&state.http_client, &goal).await;
        let resources = search_resources(&state.http_client, &goal).await;
        let entry = GoalResourceCache { intent: intent.clone(), resources: resources.clone() };
        if let Ok(s) = serde_json::to_string(&entry) {
            state.cache.put(cache_key, s, Duration::from_secs(600));
        }
        (intent, resources)
    };

    let questions = generate_questions(&goal, &intent);

    let goal_id = state.goals_state.lock().insert(goal.clone(), intent.clone(), resources);
    let created_at = {
        let store = state.goals_state.lock();
        store.get(&goal_id).map(|s| s.created_at.clone()).unwrap_or_default()
    };

    let response_json = serde_json::json!({
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

    (StatusCode::OK, Json(response_json)).into_response()
}

/// POST /goals/{goal_id}/answers — submit answers, get full roadmap
pub async fn handle_submit_answers(
    State(state): State<Arc<crate::AppState>>,
    Path(goal_id): Path<String>,
    AppJson(payload): AppJson<AnswerSubmission>,
) -> Response {
    if payload.answers.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "invalid_answers",
            "message": "Answers array cannot be empty. Submit answers for question_id 1 and 2."
        }))).into_response();
    }

    let stored = {
        let store = state.goals_state.lock();
        store.get(&goal_id).cloned()
    };

    match stored {
        Some(stored) => {
            let roadmap = generate_roadmap(&stored.goal, &payload.answers, &stored.resources);
            let total = roadmap.phases.len();

            let updated = state.goals_state.lock().update_roadmap(&goal_id, roadmap.clone());

            if updated {
                let current_stored = state.goals_state.lock().get(&goal_id).cloned();
                let score = current_stored.as_ref().map(|s| s.score).unwrap_or(0);
                let completed = current_stored.as_ref().map(|s| s.completed_phases).unwrap_or(0);
                let final_roadmap = current_stored.as_ref().and_then(|s| s.roadmap.clone()).unwrap_or(roadmap);

                (StatusCode::OK, Json(serde_json::json!({
                    "goal_id": goal_id,
                    "goal": stored.goal,
                    "intent": stored.intent,
                    "roadmap": final_roadmap,
                    "created_at": stored.created_at,
                    "status": "active",
                    "completed_phases": completed,
                    "total_phases": total,
                    "score": score
                }))).into_response()
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

/// GET /goals/{goal_id} — get goal status and roadmap
pub async fn handle_get_goal(
    State(state): State<Arc<crate::AppState>>,
    Path(goal_id): Path<String>,
) -> Response {
    let store = state.goals_state.lock();
    match store.get(&goal_id) {
        Some(s) => {
            let status = if s.roadmap.is_none() {
                "pending_answers".to_string()
            } else if s.completed_phases == s.total_phases && s.total_phases > 0 {
                "completed".to_string()
            } else {
                "active".to_string()
            };

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

            (StatusCode::OK, Json(resp)).into_response()
        }
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "not_found",
            "message": format!("Goal '{}' not found", goal_id)
        }))).into_response(),
    }
}

/// GET /goals/leaderboard — get leaderboard
pub async fn handle_leaderboard(
    State(state): State<Arc<crate::AppState>>,
) -> Response {
    let store = state.goals_state.lock();
    let entries = store.leaderboard(50);
    (StatusCode::OK, Json(serde_json::json!({
        "entries": entries,
        "total_entries": entries.len()
    }))).into_response()
}

/// POST /goals/quick — one-shot goal to full roadmap (no questions)
pub async fn handle_quick_roadmap(
    State(state): State<Arc<crate::AppState>>,
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
            let intent = classify_goal(&state.http_client, &goal).await;
            let resources = search_resources(&state.http_client, &goal).await;
            let entry = GoalResourceCache { intent: intent.clone(), resources: resources.clone() };
            if let Ok(s) = serde_json::to_string(&entry) {
                state.cache.put(cache_key, s, Duration::from_secs(600));
            }
            (intent, resources)
        }
    } else {
        let intent = classify_goal(&state.http_client, &goal).await;
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

    let goal_id = state.goals_state.lock().insert(goal.clone(), intent.clone(), resources);
    state.goals_state.lock().update_roadmap(&goal_id, roadmap.clone());
    let stored = {
        let store = state.goals_state.lock();
        store.get(&goal_id).cloned()
    };

    let response_json = serde_json::json!({
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

    (StatusCode::OK, Json(response_json)).into_response()
}

/// POST /goals/{goal_id}/phases/{phase_id}/complete — mark a phase completed
pub async fn handle_complete_phase(
    State(state): State<Arc<crate::AppState>>,
    Path((goal_id, phase_id)): Path<(String, usize)>,
) -> Response {
    let result = state.goals_state.lock().complete_phase(&goal_id, phase_id);
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
    Path(goal_id): Path<String>,
    AppJson(payload): AppJson<PhaseProgressRequest>,
) -> Response {
    let is_completed = payload.is_completed.unwrap_or(true);
    let result = state.goals_state.lock().set_phase_status(&goal_id, payload.phase_id, is_completed);
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
