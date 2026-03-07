/* ------------------------------------------------------------------ */
/*  Tool Definition Registry                                          */
/* ------------------------------------------------------------------ */

export interface ToolParameter {
  name: string;
  type: "string" | "number" | "boolean" | "object" | "array";
  description: string;
  required?: boolean;
  enum?: string[];
}

export interface ToolDef {
  id: string;
  name: string;
  description: string;
  category: "document" | "code" | "infrastructure" | "data" | "web" | "ai" | "communication" | "filesystem" | "search" | "media";
  parameters: ToolParameter[];
  /** Whether this is a user-created custom tool */
  isCustom?: boolean;
}

/* ------------------------------------------------------------------ */
/*  Built-in Tools                                                    */
/* ------------------------------------------------------------------ */

export const TOOL_CATEGORIES = [
  { id: "document" as const, label: "Document", color: "#3b82f6" },
  { id: "code" as const, label: "Code", color: "#22c55e" },
  { id: "infrastructure" as const, label: "Infrastructure", color: "#a855f7" },
  { id: "data" as const, label: "Data", color: "#10b981" },
  { id: "web" as const, label: "Web", color: "#06b6d4" },
  { id: "ai" as const, label: "AI / ML", color: "#f59e0b" },
  { id: "communication" as const, label: "Communication", color: "#ec4899" },
  { id: "filesystem" as const, label: "Filesystem", color: "#6b7280" },
  { id: "search" as const, label: "Search", color: "#8b5cf6" },
  { id: "media" as const, label: "Media", color: "#f97316" },
] as const;

export type ToolCategoryId = (typeof TOOL_CATEGORIES)[number]["id"];

const builtInTools: ToolDef[] = [
  /* ---- Document Tools ---- */
  {
    id: "read_document",
    name: "Read Document",
    description: "Read the contents of a document by ID or path, returning its text, metadata, and structure.",
    category: "document",
    parameters: [
      { name: "document_id", type: "string", description: "The document ID or path to read", required: true },
      { name: "format", type: "string", description: "Output format", enum: ["text", "markdown", "html", "json"] },
    ],
  },
  {
    id: "write_document",
    name: "Write Document",
    description: "Create or overwrite a document with the given content and metadata.",
    category: "document",
    parameters: [
      { name: "title", type: "string", description: "Document title", required: true },
      { name: "content", type: "string", description: "Document content in markdown", required: true },
      { name: "folder", type: "string", description: "Target folder path" },
      { name: "tags", type: "array", description: "Tags to apply to the document" },
    ],
  },
  {
    id: "edit_document",
    name: "Edit Document",
    description: "Apply targeted edits to an existing document — insert, replace, or delete sections.",
    category: "document",
    parameters: [
      { name: "document_id", type: "string", description: "The document to edit", required: true },
      { name: "operation", type: "string", description: "Edit operation", required: true, enum: ["insert", "replace", "delete", "append"] },
      { name: "target", type: "string", description: "Section heading or line range to target" },
      { name: "content", type: "string", description: "New content to insert or replace with" },
    ],
  },
  {
    id: "convert_format",
    name: "Convert Format",
    description: "Convert a document between formats (Markdown, HTML, PDF, DOCX, LaTeX, EPUB).",
    category: "document",
    parameters: [
      { name: "document_id", type: "string", description: "Source document", required: true },
      { name: "target_format", type: "string", description: "Target format", required: true, enum: ["markdown", "html", "pdf", "docx", "latex", "epub", "txt"] },
      { name: "options", type: "object", description: "Format-specific options (e.g., page size, font)" },
    ],
  },
  {
    id: "search_documents",
    name: "Search Documents",
    description: "Search across all documents using full-text or semantic search, returning ranked results.",
    category: "document",
    parameters: [
      { name: "query", type: "string", description: "Search query", required: true },
      { name: "mode", type: "string", description: "Search mode", enum: ["fulltext", "semantic", "regex"] },
      { name: "limit", type: "number", description: "Maximum results to return" },
      { name: "filters", type: "object", description: "Filter by tags, date range, folder, etc." },
    ],
  },
  {
    id: "extract_text",
    name: "Extract Text",
    description: "Extract plain text from PDFs, images (OCR), DOCX, or scanned documents.",
    category: "document",
    parameters: [
      { name: "source", type: "string", description: "File path or URL to extract from", required: true },
      { name: "language", type: "string", description: "Language hint for OCR" },
      { name: "pages", type: "string", description: "Page range (e.g., '1-5', 'all')" },
    ],
  },
  {
    id: "apply_template",
    name: "Apply Template",
    description: "Fill a document template with provided data, replacing placeholders and conditionals.",
    category: "document",
    parameters: [
      { name: "template_id", type: "string", description: "Template document ID", required: true },
      { name: "data", type: "object", description: "Key-value data to fill into the template", required: true },
      { name: "output_title", type: "string", description: "Title for the generated document" },
    ],
  },
  {
    id: "diff_documents",
    name: "Diff Documents",
    description: "Compare two document versions and produce a structured diff with additions, deletions, and changes.",
    category: "document",
    parameters: [
      { name: "document_a", type: "string", description: "First document ID or version", required: true },
      { name: "document_b", type: "string", description: "Second document ID or version", required: true },
      { name: "granularity", type: "string", description: "Diff granularity", enum: ["word", "line", "paragraph", "section"] },
    ],
  },
  {
    id: "merge_documents",
    name: "Merge Documents",
    description: "Merge multiple documents into one, with configurable section ordering and deduplication.",
    category: "document",
    parameters: [
      { name: "document_ids", type: "array", description: "List of document IDs to merge", required: true },
      { name: "strategy", type: "string", description: "Merge strategy", enum: ["concatenate", "interleave", "smart"] },
      { name: "output_title", type: "string", description: "Title for the merged document" },
    ],
  },
  {
    id: "redact_content",
    name: "Redact Content",
    description: "Redact sensitive information (PII, secrets, confidential data) from a document.",
    category: "document",
    parameters: [
      { name: "document_id", type: "string", description: "Document to redact", required: true },
      { name: "patterns", type: "array", description: "Types to redact: email, phone, ssn, credit_card, custom regex" },
      { name: "replacement", type: "string", description: "Replacement text (default: [REDACTED])" },
    ],
  },

  /* ---- Code Tools ---- */
  {
    id: "read_file",
    name: "Read File",
    description: "Read source code or configuration files from the project, with optional line range.",
    category: "code",
    parameters: [
      { name: "path", type: "string", description: "File path relative to project root", required: true },
      { name: "start_line", type: "number", description: "Start line (1-based)" },
      { name: "end_line", type: "number", description: "End line (inclusive)" },
    ],
  },
  {
    id: "write_file",
    name: "Write File",
    description: "Create or overwrite a source file with the given content.",
    category: "code",
    parameters: [
      { name: "path", type: "string", description: "File path relative to project root", required: true },
      { name: "content", type: "string", description: "File content", required: true },
    ],
  },
  {
    id: "edit_file",
    name: "Edit File",
    description: "Apply targeted edits to a source file using search-and-replace.",
    category: "code",
    parameters: [
      { name: "path", type: "string", description: "File path", required: true },
      { name: "old_text", type: "string", description: "Text to find", required: true },
      { name: "new_text", type: "string", description: "Replacement text", required: true },
    ],
  },
  {
    id: "run_command",
    name: "Run Command",
    description: "Execute a shell command in the project directory and return stdout/stderr.",
    category: "code",
    parameters: [
      { name: "command", type: "string", description: "Shell command to execute", required: true },
      { name: "working_dir", type: "string", description: "Working directory (defaults to project root)" },
      { name: "timeout", type: "number", description: "Timeout in seconds" },
    ],
  },
  {
    id: "run_tests",
    name: "Run Tests",
    description: "Execute test suites and return results with pass/fail counts and failure details.",
    category: "code",
    parameters: [
      { name: "pattern", type: "string", description: "Test file pattern or specific test name" },
      { name: "framework", type: "string", description: "Test framework", enum: ["jest", "vitest", "pytest", "go-test", "cargo-test"] },
      { name: "coverage", type: "boolean", description: "Whether to collect coverage data" },
    ],
  },
  {
    id: "lint_code",
    name: "Lint Code",
    description: "Run linters and static analysis on source files, returning issues with severity and fix suggestions.",
    category: "code",
    parameters: [
      { name: "paths", type: "array", description: "Files or directories to lint" },
      { name: "fix", type: "boolean", description: "Auto-fix issues where possible" },
      { name: "rules", type: "array", description: "Specific rules to check" },
    ],
  },
  {
    id: "search_code",
    name: "Search Code",
    description: "Search codebase using text, regex, or AST patterns across files.",
    category: "code",
    parameters: [
      { name: "query", type: "string", description: "Search query or pattern", required: true },
      { name: "mode", type: "string", description: "Search mode", enum: ["text", "regex", "ast", "semantic"] },
      { name: "file_pattern", type: "string", description: "Glob pattern to filter files (e.g., '**/*.ts')" },
      { name: "limit", type: "number", description: "Max results" },
    ],
  },
  {
    id: "git_operations",
    name: "Git Operations",
    description: "Perform git operations: status, diff, log, branch, commit, push, pull.",
    category: "code",
    parameters: [
      { name: "operation", type: "string", description: "Git operation", required: true, enum: ["status", "diff", "log", "branch", "commit", "push", "pull", "stash", "checkout"] },
      { name: "args", type: "string", description: "Additional arguments for the git command" },
      { name: "message", type: "string", description: "Commit message (for commit operation)" },
    ],
  },
  {
    id: "refactor_code",
    name: "Refactor Code",
    description: "Apply automated refactoring operations: rename, extract function, move, inline.",
    category: "code",
    parameters: [
      { name: "operation", type: "string", description: "Refactoring type", required: true, enum: ["rename", "extract_function", "extract_variable", "inline", "move", "convert"] },
      { name: "target", type: "string", description: "Symbol or code range to refactor", required: true },
      { name: "new_name", type: "string", description: "New name (for rename)" },
      { name: "destination", type: "string", description: "Destination file (for move)" },
    ],
  },
  {
    id: "analyze_dependencies",
    name: "Analyze Dependencies",
    description: "Analyze project dependencies: versions, vulnerabilities, licenses, unused packages.",
    category: "code",
    parameters: [
      { name: "check", type: "string", description: "Analysis type", enum: ["outdated", "vulnerabilities", "licenses", "unused", "duplicates"] },
      { name: "package_manager", type: "string", description: "Package manager", enum: ["npm", "yarn", "pnpm", "pip", "cargo", "go"] },
    ],
  },
  {
    id: "generate_types",
    name: "Generate Types",
    description: "Generate TypeScript types, interfaces, or schemas from data samples, APIs, or databases.",
    category: "code",
    parameters: [
      { name: "source", type: "string", description: "Source to generate types from", required: true },
      { name: "source_type", type: "string", description: "Source type", required: true, enum: ["json_sample", "api_endpoint", "database_schema", "graphql", "protobuf"] },
      { name: "output_path", type: "string", description: "Where to write generated types" },
    ],
  },

  /* ---- Infrastructure Tools ---- */
  {
    id: "deploy",
    name: "Deploy",
    description: "Deploy application or infrastructure changes to a target environment.",
    category: "infrastructure",
    parameters: [
      { name: "target", type: "string", description: "Deployment target", required: true, enum: ["development", "staging", "production", "preview"] },
      { name: "service", type: "string", description: "Service or stack to deploy" },
      { name: "strategy", type: "string", description: "Deployment strategy", enum: ["rolling", "blue-green", "canary", "recreate"] },
      { name: "dry_run", type: "boolean", description: "Preview changes without applying" },
    ],
  },
  {
    id: "terraform_plan",
    name: "Terraform Plan",
    description: "Run terraform plan to preview infrastructure changes before applying.",
    category: "infrastructure",
    parameters: [
      { name: "workspace", type: "string", description: "Terraform workspace" },
      { name: "targets", type: "array", description: "Specific resources to plan" },
      { name: "var_file", type: "string", description: "Variables file path" },
    ],
  },
  {
    id: "terraform_apply",
    name: "Terraform Apply",
    description: "Apply terraform changes to provision or update infrastructure.",
    category: "infrastructure",
    parameters: [
      { name: "workspace", type: "string", description: "Terraform workspace" },
      { name: "auto_approve", type: "boolean", description: "Skip interactive approval" },
      { name: "targets", type: "array", description: "Specific resources to apply" },
    ],
  },
  {
    id: "docker_build",
    name: "Docker Build",
    description: "Build a Docker image from a Dockerfile with configurable build args and tags.",
    category: "infrastructure",
    parameters: [
      { name: "dockerfile", type: "string", description: "Path to Dockerfile", required: true },
      { name: "tag", type: "string", description: "Image tag", required: true },
      { name: "build_args", type: "object", description: "Build arguments" },
      { name: "platform", type: "string", description: "Target platform (e.g., linux/amd64)" },
    ],
  },
  {
    id: "kubectl",
    name: "Kubectl",
    description: "Run kubectl commands to manage Kubernetes resources.",
    category: "infrastructure",
    parameters: [
      { name: "command", type: "string", description: "kubectl subcommand", required: true, enum: ["get", "describe", "apply", "delete", "logs", "exec", "rollout", "scale"] },
      { name: "resource", type: "string", description: "Resource type and name (e.g., 'pods/my-pod')" },
      { name: "namespace", type: "string", description: "Kubernetes namespace" },
      { name: "flags", type: "string", description: "Additional flags" },
    ],
  },
  {
    id: "check_health",
    name: "Check Health",
    description: "Check the health status of services, endpoints, databases, and infrastructure components.",
    category: "infrastructure",
    parameters: [
      { name: "target", type: "string", description: "Service URL, hostname, or resource identifier", required: true },
      { name: "check_type", type: "string", description: "Type of health check", enum: ["http", "tcp", "dns", "database", "certificate"] },
      { name: "timeout", type: "number", description: "Timeout in seconds" },
    ],
  },
  {
    id: "query_metrics",
    name: "Query Metrics",
    description: "Query monitoring metrics from Prometheus, CloudWatch, Datadog, or similar systems.",
    category: "infrastructure",
    parameters: [
      { name: "query", type: "string", description: "Metrics query (PromQL, CloudWatch, etc.)", required: true },
      { name: "time_range", type: "string", description: "Time range (e.g., '1h', '24h', '7d')" },
      { name: "source", type: "string", description: "Metrics source", enum: ["prometheus", "cloudwatch", "datadog", "grafana"] },
    ],
  },
  {
    id: "query_logs",
    name: "Query Logs",
    description: "Search and filter application and infrastructure logs.",
    category: "infrastructure",
    parameters: [
      { name: "query", type: "string", description: "Log search query", required: true },
      { name: "source", type: "string", description: "Log source", enum: ["cloudwatch", "elasticsearch", "loki", "stackdriver", "local"] },
      { name: "time_range", type: "string", description: "Time range" },
      { name: "severity", type: "string", description: "Minimum severity", enum: ["debug", "info", "warn", "error", "fatal"] },
    ],
  },
  {
    id: "manage_secrets",
    name: "Manage Secrets",
    description: "Read, write, or rotate secrets in a secrets manager (Vault, AWS SM, etc.).",
    category: "infrastructure",
    parameters: [
      { name: "operation", type: "string", description: "Secret operation", required: true, enum: ["get", "set", "delete", "rotate", "list"] },
      { name: "key", type: "string", description: "Secret key/path", required: true },
      { name: "value", type: "string", description: "Secret value (for set)" },
      { name: "backend", type: "string", description: "Secrets backend", enum: ["vault", "aws-sm", "azure-kv", "gcp-sm", "env"] },
    ],
  },
  {
    id: "cost_estimate",
    name: "Cost Estimate",
    description: "Estimate cloud infrastructure costs for a given configuration or terraform plan.",
    category: "infrastructure",
    parameters: [
      { name: "source", type: "string", description: "Terraform plan file or resource spec", required: true },
      { name: "provider", type: "string", description: "Cloud provider", enum: ["aws", "azure", "gcp"] },
      { name: "period", type: "string", description: "Cost period", enum: ["hourly", "daily", "monthly", "yearly"] },
    ],
  },

  /* ---- Data Tools ---- */
  {
    id: "query_database",
    name: "Query Database",
    description: "Execute SQL or NoSQL queries against a database and return results.",
    category: "data",
    parameters: [
      { name: "query", type: "string", description: "Database query", required: true },
      { name: "connection", type: "string", description: "Database connection name or URI", required: true },
      { name: "limit", type: "number", description: "Max rows to return" },
    ],
  },
  {
    id: "transform_data",
    name: "Transform Data",
    description: "Transform data between formats (JSON, CSV, XML, YAML) with mapping and filtering.",
    category: "data",
    parameters: [
      { name: "input", type: "string", description: "Input data or file path", required: true },
      { name: "input_format", type: "string", description: "Input format", required: true, enum: ["json", "csv", "xml", "yaml", "tsv"] },
      { name: "output_format", type: "string", description: "Output format", required: true, enum: ["json", "csv", "xml", "yaml", "tsv"] },
      { name: "mapping", type: "object", description: "Field mapping rules" },
    ],
  },
  {
    id: "validate_data",
    name: "Validate Data",
    description: "Validate data against a schema (JSON Schema, XML Schema, custom rules).",
    category: "data",
    parameters: [
      { name: "data", type: "string", description: "Data to validate", required: true },
      { name: "schema", type: "string", description: "Schema to validate against", required: true },
      { name: "schema_type", type: "string", description: "Schema type", enum: ["json-schema", "xml-schema", "avro", "protobuf", "custom"] },
    ],
  },
  {
    id: "analyze_data",
    name: "Analyze Data",
    description: "Perform statistical analysis on a dataset: distributions, correlations, outliers, trends.",
    category: "data",
    parameters: [
      { name: "data", type: "string", description: "Data source (file path, query result, or inline)", required: true },
      { name: "analysis", type: "string", description: "Analysis type", required: true, enum: ["summary", "distribution", "correlation", "outliers", "trends", "clustering"] },
      { name: "columns", type: "array", description: "Columns to analyze" },
    ],
  },
  {
    id: "generate_chart",
    name: "Generate Chart",
    description: "Generate a chart or visualization from data (bar, line, pie, scatter, heatmap).",
    category: "data",
    parameters: [
      { name: "data", type: "string", description: "Data source", required: true },
      { name: "chart_type", type: "string", description: "Chart type", required: true, enum: ["bar", "line", "pie", "scatter", "heatmap", "histogram", "treemap"] },
      { name: "title", type: "string", description: "Chart title" },
      { name: "x_axis", type: "string", description: "X-axis column" },
      { name: "y_axis", type: "string", description: "Y-axis column" },
    ],
  },

  /* ---- Web Tools ---- */
  {
    id: "http_request",
    name: "HTTP Request",
    description: "Make HTTP requests to APIs and return the response with headers and body.",
    category: "web",
    parameters: [
      { name: "url", type: "string", description: "Request URL", required: true },
      { name: "method", type: "string", description: "HTTP method", enum: ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"] },
      { name: "headers", type: "object", description: "Request headers" },
      { name: "body", type: "string", description: "Request body" },
    ],
  },
  {
    id: "scrape_webpage",
    name: "Scrape Webpage",
    description: "Extract structured content from a webpage — text, links, tables, images, metadata.",
    category: "web",
    parameters: [
      { name: "url", type: "string", description: "URL to scrape", required: true },
      { name: "selectors", type: "object", description: "CSS selectors for specific content extraction" },
      { name: "format", type: "string", description: "Output format", enum: ["text", "markdown", "json", "html"] },
    ],
  },
  {
    id: "web_search",
    name: "Web Search",
    description: "Search the web and return ranked results with titles, URLs, and snippets.",
    category: "web",
    parameters: [
      { name: "query", type: "string", description: "Search query", required: true },
      { name: "limit", type: "number", description: "Max results" },
      { name: "site", type: "string", description: "Restrict to a specific site" },
    ],
  },

  /* ---- AI Tools ---- */
  {
    id: "llm_generate",
    name: "LLM Generate",
    description: "Generate text using an LLM with a custom prompt, temperature, and model selection.",
    category: "ai",
    parameters: [
      { name: "prompt", type: "string", description: "The prompt to send", required: true },
      { name: "model", type: "string", description: "Model to use" },
      { name: "temperature", type: "number", description: "Temperature (0-1)" },
      { name: "max_tokens", type: "number", description: "Maximum tokens to generate" },
      { name: "system", type: "string", description: "System prompt override" },
    ],
  },
  {
    id: "embedding_generate",
    name: "Generate Embedding",
    description: "Generate vector embeddings for text, used for semantic search and similarity.",
    category: "ai",
    parameters: [
      { name: "text", type: "string", description: "Text to embed", required: true },
      { name: "model", type: "string", description: "Embedding model" },
    ],
  },
  {
    id: "classify_text",
    name: "Classify Text",
    description: "Classify text into categories using a trained model or zero-shot classification.",
    category: "ai",
    parameters: [
      { name: "text", type: "string", description: "Text to classify", required: true },
      { name: "labels", type: "array", description: "Possible category labels", required: true },
      { name: "multi_label", type: "boolean", description: "Allow multiple labels" },
    ],
  },
  {
    id: "summarize",
    name: "Summarize",
    description: "Generate a summary of text content at a specified length and style.",
    category: "ai",
    parameters: [
      { name: "text", type: "string", description: "Text to summarize", required: true },
      { name: "max_length", type: "number", description: "Maximum summary length in words" },
      { name: "style", type: "string", description: "Summary style", enum: ["executive", "technical", "bullet_points", "abstract", "tldr"] },
    ],
  },
  {
    id: "translate",
    name: "Translate",
    description: "Translate text between languages with optional glossary and tone settings.",
    category: "ai",
    parameters: [
      { name: "text", type: "string", description: "Text to translate", required: true },
      { name: "source_lang", type: "string", description: "Source language (auto-detect if omitted)" },
      { name: "target_lang", type: "string", description: "Target language", required: true },
      { name: "tone", type: "string", description: "Translation tone", enum: ["formal", "informal", "technical", "legal"] },
    ],
  },

  /* ---- Communication Tools ---- */
  {
    id: "send_notification",
    name: "Send Notification",
    description: "Send a notification via email, Slack, Teams, or webhook.",
    category: "communication",
    parameters: [
      { name: "channel", type: "string", description: "Notification channel", required: true, enum: ["email", "slack", "teams", "webhook", "sms"] },
      { name: "recipient", type: "string", description: "Recipient address or channel", required: true },
      { name: "subject", type: "string", description: "Notification subject" },
      { name: "message", type: "string", description: "Notification body", required: true },
    ],
  },
  {
    id: "create_ticket",
    name: "Create Ticket",
    description: "Create a ticket/issue in a project management tool (Jira, Linear, GitHub Issues).",
    category: "communication",
    parameters: [
      { name: "title", type: "string", description: "Ticket title", required: true },
      { name: "description", type: "string", description: "Ticket description", required: true },
      { name: "project", type: "string", description: "Project key or ID", required: true },
      { name: "priority", type: "string", description: "Priority level", enum: ["critical", "high", "medium", "low"] },
      { name: "assignee", type: "string", description: "Assignee username or ID" },
      { name: "labels", type: "array", description: "Labels/tags to apply" },
    ],
  },

  /* ---- Filesystem Tools ---- */
  {
    id: "list_files",
    name: "List Files",
    description: "List files and directories matching a glob pattern.",
    category: "filesystem",
    parameters: [
      { name: "path", type: "string", description: "Directory path", required: true },
      { name: "pattern", type: "string", description: "Glob pattern to filter" },
      { name: "recursive", type: "boolean", description: "Include subdirectories" },
    ],
  },
  {
    id: "copy_file",
    name: "Copy File",
    description: "Copy a file or directory to a new location.",
    category: "filesystem",
    parameters: [
      { name: "source", type: "string", description: "Source path", required: true },
      { name: "destination", type: "string", description: "Destination path", required: true },
      { name: "overwrite", type: "boolean", description: "Overwrite if destination exists" },
    ],
  },
  {
    id: "move_file",
    name: "Move File",
    description: "Move or rename a file or directory.",
    category: "filesystem",
    parameters: [
      { name: "source", type: "string", description: "Source path", required: true },
      { name: "destination", type: "string", description: "Destination path", required: true },
    ],
  },
  {
    id: "archive_files",
    name: "Archive Files",
    description: "Create or extract archives (zip, tar.gz, tar.bz2).",
    category: "filesystem",
    parameters: [
      { name: "operation", type: "string", description: "Archive operation", required: true, enum: ["create", "extract", "list"] },
      { name: "archive_path", type: "string", description: "Archive file path", required: true },
      { name: "files", type: "array", description: "Files to include (for create)" },
      { name: "format", type: "string", description: "Archive format", enum: ["zip", "tar.gz", "tar.bz2", "7z"] },
    ],
  },

  /* ---- Search Tools ---- */
  {
    id: "semantic_search",
    name: "Semantic Search",
    description: "Search using vector embeddings for semantic similarity rather than keyword matching.",
    category: "search",
    parameters: [
      { name: "query", type: "string", description: "Natural language search query", required: true },
      { name: "collection", type: "string", description: "Vector collection to search", required: true },
      { name: "limit", type: "number", description: "Max results" },
      { name: "threshold", type: "number", description: "Minimum similarity threshold (0-1)" },
    ],
  },
  {
    id: "regex_search",
    name: "Regex Search",
    description: "Search content using regular expressions across files or documents.",
    category: "search",
    parameters: [
      { name: "pattern", type: "string", description: "Regex pattern", required: true },
      { name: "scope", type: "string", description: "Search scope", enum: ["documents", "codebase", "logs", "all"] },
      { name: "limit", type: "number", description: "Max results" },
    ],
  },

  /* ---- Media Tools ---- */
  {
    id: "generate_image",
    name: "Generate Image",
    description: "Generate an image from a text description using an image generation model.",
    category: "media",
    parameters: [
      { name: "prompt", type: "string", description: "Image description", required: true },
      { name: "size", type: "string", description: "Image size", enum: ["256x256", "512x512", "1024x1024", "1024x1792", "1792x1024"] },
      { name: "style", type: "string", description: "Image style", enum: ["natural", "vivid", "artistic", "photographic"] },
    ],
  },
  {
    id: "generate_diagram",
    name: "Generate Diagram",
    description: "Generate a diagram from a text description or structured data (flowcharts, sequence, ER, etc.).",
    category: "media",
    parameters: [
      { name: "description", type: "string", description: "Diagram description or DSL", required: true },
      { name: "type", type: "string", description: "Diagram type", required: true, enum: ["flowchart", "sequence", "class", "er", "state", "architecture", "network"] },
      { name: "format", type: "string", description: "Output format", enum: ["svg", "png", "mermaid", "d2"] },
    ],
  },
  {
    id: "resize_image",
    name: "Resize Image",
    description: "Resize, crop, or transform an image.",
    category: "media",
    parameters: [
      { name: "source", type: "string", description: "Source image path or URL", required: true },
      { name: "width", type: "number", description: "Target width" },
      { name: "height", type: "number", description: "Target height" },
      { name: "operation", type: "string", description: "Operation", enum: ["resize", "crop", "rotate", "flip", "compress"] },
    ],
  },
];

/* ------------------------------------------------------------------ */
/*  Custom tools store (persisted to localStorage)                    */
/* ------------------------------------------------------------------ */

const CUSTOM_TOOLS_KEY = "celerex-custom-tools";

function loadCustomTools(): ToolDef[] {
  try {
    const raw = localStorage.getItem(CUSTOM_TOOLS_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function saveCustomTools(tools: ToolDef[]) {
  localStorage.setItem(CUSTOM_TOOLS_KEY, JSON.stringify(tools));
}

let customTools: ToolDef[] = loadCustomTools();

/* ------------------------------------------------------------------ */
/*  Public API                                                        */
/* ------------------------------------------------------------------ */

export function getAllTools(): ToolDef[] {
  return [...builtInTools, ...customTools];
}

export function getToolById(id: string): ToolDef | undefined {
  return getAllTools().find((t) => t.id === id);
}

export function getToolsByCategory(category: ToolCategoryId): ToolDef[] {
  return getAllTools().filter((t) => t.category === category);
}

export function getToolsByIds(ids: string[]): ToolDef[] {
  return ids.map((id) => getToolById(id)).filter(Boolean) as ToolDef[];
}

export function addCustomTool(tool: ToolDef): void {
  customTools = [...customTools, { ...tool, isCustom: true }];
  saveCustomTools(customTools);
}

export function updateCustomTool(id: string, updates: Partial<ToolDef>): void {
  customTools = customTools.map((t) => (t.id === id ? { ...t, ...updates } : t));
  saveCustomTools(customTools);
}

export function deleteCustomTool(id: string): void {
  customTools = customTools.filter((t) => t.id !== id);
  saveCustomTools(customTools);
}

export function searchTools(query: string, category?: ToolCategoryId): ToolDef[] {
  const q = query.toLowerCase().trim();
  let tools = category ? getToolsByCategory(category) : getAllTools();
  if (!q) return tools;
  return tools.filter(
    (t) =>
      t.name.toLowerCase().includes(q) ||
      t.description.toLowerCase().includes(q) ||
      t.id.includes(q)
  );
}
