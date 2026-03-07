import type { CategoryId } from "./agents";

export interface Subcategory {
  id: string;
  label: string;
  keywords: string[]; // used to auto-classify agents
}

export const SUBCATEGORIES: Record<CategoryId, Subcategory[]> = {
  "document-management": [
    { id: "creation-templates", label: "Creation & Templates", keywords: ["template", "generator", "scaffolder", "drafter", "builder", "creator", "boilerplate", "memo", "proposal", "letter", "invoice", "report", "sop", "faq", "onboarding", "changelog", "minutes"] },
    { id: "formatting-styling", label: "Formatting & Styling", keywords: ["format", "typography", "layout", "style", "theme", "css-to-doc", "brand", "table", "heading", "list", "page break", "citation", "dark mode", "footnote", "switcher"] },
    { id: "content-analysis", label: "Content Analysis", keywords: ["readability", "tone", "sentiment", "complexity", "bias", "jargon", "plagiarism", "consistency", "frequency", "audience", "fact claim", "passive voice", "gender neutral", "argument", "data reference"] },
    { id: "summarization-extraction", label: "Summarization & Extraction", keywords: ["summar", "extract", "tl;dr", "key point", "entity", "toc", "outline", "abstract", "digest", "condense", "highlight", "brief"] },
    { id: "translation-localization", label: "Translation & Localization", keywords: ["translat", "localiz", "language", "multilingual", "glossary", "cultural", "rtl", "unicode", "accent", "dialect", "terminology"] },
    { id: "version-history", label: "Version Control & History", keywords: ["version", "diff", "merge", "changelog", "branch", "history", "revision", "track change", "snapshot", "rollback"] },
    { id: "collaboration-review", label: "Collaboration & Review", keywords: ["collaborat", "review", "comment", "approval", "track change", "co-edit", "annotati", "feedback", "peer", "sign-off"] },
    { id: "import-export", label: "Import / Export & Conversion", keywords: ["import", "export", "convert", "pdf", "docx", "html", "latex", "epub", "markdown", "csv", "xml", "migration"] },
    { id: "compliance-legal", label: "Compliance & Legal", keywords: ["complian", "legal", "gdpr", "accessib", "contract", "redact", "regulat", "hipaa", "pci", "audit", "policy", "privacy", "retain"] },
    { id: "search-discovery", label: "Search & Discovery", keywords: ["search", "tag", "classif", "deduplic", "index", "catalog", "discover", "semantic", "similar", "recommend"] },
    { id: "content-generation", label: "Content Generation", keywords: ["generat", "write", "blog", "press release", "email", "newsletter", "social media", "copywriting", "content"] },
    { id: "editing-proofreading", label: "Editing & Proofreading", keywords: ["grammar", "spell", "proofread", "edit", "style guide", "ap style", "chicago", "punctuat", "word choice"] },
    { id: "media-embedding", label: "Media & Embedding", keywords: ["image", "caption", "chart", "table extract", "embed", "media", "diagram", "figure", "screenshot", "alt text"] },
    { id: "archival-retention", label: "Archival & Retention", keywords: ["archiv", "retention", "backup", "expir", "cold storage", "purge", "preserve", "long-term"] },
    { id: "workflow-automation", label: "Workflow Automation", keywords: ["workflow", "automat", "routing", "notification", "batch", "trigger", "schedul", "queue", "pipeline"] },
  ],
  "co-development": [
    { id: "code-review", label: "Code Review & Analysis", keywords: ["review", "static analysis", "complexity", "duplicat", "lint", "code smell", "quality", "dead code", "coverage"] },
    { id: "testing-qa", label: "Testing & QA", keywords: ["test", "unit test", "integration", "e2e", "fuzz", "mutation", "snapshot", "regression", "flak", "assertion"] },
    { id: "debugging", label: "Debugging & Diagnostics", keywords: ["debug", "stack trace", "memory", "profil", "bottleneck", "error", "exception", "crash", "leak", "diagnos"] },
    { id: "api-development", label: "API Development", keywords: ["api", "rest", "graphql", "openapi", "swagger", "mock", "contract", "endpoint", "schema valid", "grpc"] },
    { id: "database-data", label: "Database & Data", keywords: ["database", "schema", "migration", "query", "sql", "orm", "seed data", "index", "normali"] },
    { id: "frontend-dev", label: "Frontend Development", keywords: ["react", "css", "responsive", "component", "ui", "ux", "accessibility", "a11y", "animation", "layout", "ssr", "hydrat"] },
    { id: "backend-dev", label: "Backend Development", keywords: ["microservice", "message queue", "cach", "auth", "middleware", "server", "lambda", "handler", "controller"] },
    { id: "devops-cicd", label: "DevOps & CI/CD", keywords: ["devops", "ci/cd", "pipeline", "docker", "kubernetes", "deploy", "container", "helm", "build", "artifact"] },
    { id: "security", label: "Security", keywords: ["security", "owasp", "vulnerab", "penetrat", "secret", "encrypt", "injection", "xss", "csrf", "depend"] },
    { id: "performance", label: "Performance", keywords: ["performance", "load test", "benchmark", "bundle", "optimi", "lazy", "speed", "latency", "throughput"] },
    { id: "documentation", label: "Documentation", keywords: ["doc", "readme", "jsdoc", "tsdoc", "architecture diagram", "comment", "wiki", "changelog"] },
    { id: "refactoring", label: "Refactoring", keywords: ["refactor", "pattern", "extract", "dependency injection", "decoupl", "modular", "clean code", "rename"] },
    { id: "version-control", label: "Version Control", keywords: ["git", "branch", "merge conflict", "rebase", "commit", "pr", "pull request", "cherry-pick"] },
    { id: "language-specific", label: "Language-Specific Tools", keywords: ["typescript", "rust", "python", "go", "java", "c++", "swift", "kotlin", "ruby", "php"] },
    { id: "architecture", label: "Architecture & Design", keywords: ["architect", "design pattern", "domain-driven", "event sourc", "cqrs", "microservice", "monolith", "solid"] },
    { id: "pair-programming", label: "Pair Programming & AI Assist", keywords: ["pair", "rubber duck", "suggest", "complet", "copilot", "assist", "real-time", "live"] },
    { id: "package-management", label: "Package Management", keywords: ["package", "dependency", "npm", "cargo", "pip", "license", "vulnerab", "update", "upgrade"] },
  ],
  "infrastructure-design": [
    { id: "cloud-architecture", label: "Cloud Architecture", keywords: ["cloud", "aws", "azure", "gcp", "multi-cloud", "hybrid", "serverless", "landing zone", "account", "region"] },
    { id: "networking", label: "Networking", keywords: ["vpc", "network", "load balanc", "dns", "cdn", "vpn", "firewall", "subnet", "cidr", "service mesh", "transit", "peering"] },
    { id: "compute-scaling", label: "Compute & Scaling", keywords: ["auto-scal", "spot instance", "reserved", "edge compute", "gpu", "instance type", "capacity", "bare metal", "compute"] },
    { id: "storage-databases", label: "Storage & Databases", keywords: ["storage", "database", "s3", "blob", "data lake", "replicat", "backup", "rds", "dynamo", "redis", "postgres"] },
    { id: "container-orchestration", label: "Container Orchestration", keywords: ["kubernetes", "k8s", "ecs", "docker", "helm", "pod", "container", "swarm", "fargate", "service mesh"] },
    { id: "infrastructure-as-code", label: "Infrastructure as Code", keywords: ["terraform", "pulumi", "cloudformation", "ansible", "chef", "iac", "infra-as-code", "cdk", "crossplane"] },
    { id: "monitoring-observability", label: "Monitoring & Observability", keywords: ["monitor", "metric", "log", "trac", "alert", "dashboard", "slo", "sli", "prometheus", "grafana", "observ"] },
    { id: "security-compliance", label: "Security & Compliance", keywords: ["iam", "encrypt", "network polic", "soc2", "hipaa", "pci", "compliance", "secret", "certificate", "tls", "zero trust"] },
    { id: "cost-optimization", label: "Cost Optimization", keywords: ["cost", "right-siz", "budget", "savings", "billing", "finops", "waste", "utiliz", "pricing"] },
    { id: "disaster-recovery", label: "Disaster Recovery", keywords: ["disaster", "failover", "rto", "rpo", "chaos", "backup", "recovery", "resilien", "redundan"] },
    { id: "cicd-infrastructure", label: "CI/CD Infrastructure", keywords: ["build farm", "artifact registr", "gitops", "deploy pipeline", "release", "canary", "blue-green", "rolling"] },
    { id: "api-infrastructure", label: "API Infrastructure", keywords: ["api gateway", "rate limit", "service discover", "circuit breaker", "throttl", "proxy", "ingress"] },
    { id: "data-infrastructure", label: "Data Infrastructure", keywords: ["etl", "streaming", "data warehouse", "lakehouse", "kafka", "spark", "airflow", "pipeline", "batch process"] },
    { id: "edge-iot", label: "Edge & IoT", keywords: ["edge", "iot", "device", "fleet", "mqtt", "protocol", "sensor", "gateway", "fog"] },
    { id: "platform-engineering", label: "Platform Engineering", keywords: ["developer portal", "golden path", "internal tool", "self-service", "platform", "scaffol", "template"] },
    { id: "performance-engineering", label: "Performance Engineering", keywords: ["capacity plan", "benchmark", "traffic shap", "caching layer", "tuning", "profil", "stress"] },
    { id: "migration-modernization", label: "Migration & Modernization", keywords: ["migrat", "moderniz", "lift-and-shift", "re-architect", "strangler", "legacy", "decompos"] },
  ],
};

/**
 * Auto-classify an agent into a subcategory based on its name and description.
 */
export function classifyAgent(
  agentName: string,
  agentDescription: string,
  category: CategoryId
): string {
  const text = `${agentName} ${agentDescription}`.toLowerCase();
  const subs = SUBCATEGORIES[category];
  if (!subs) return "other";

  let bestMatch = "";
  let bestScore = 0;

  for (const sub of subs) {
    let score = 0;
    for (const kw of sub.keywords) {
      if (text.includes(kw.toLowerCase())) {
        score += kw.length; // longer keyword matches are more specific
      }
    }
    if (score > bestScore) {
      bestScore = score;
      bestMatch = sub.id;
    }
  }

  return bestMatch || "other";
}

/**
 * Get subcategory label
 */
export function getSubcategoryLabel(category: CategoryId, subcategoryId: string): string {
  const sub = SUBCATEGORIES[category]?.find((s) => s.id === subcategoryId);
  return sub?.label ?? "Other";
}
