/* ------------------------------------------------------------------ */
/*  Agent Triggers — data model & persistence                         */
/* ------------------------------------------------------------------ */

export type TriggerType = "schedule" | "webhook" | "file_watch" | "event" | "email" | "git" | "queue" | "manual" | "rss" | "database" | "chat" | "calendar" | "api_poll" | "threshold" | "mqtt";

export interface ScheduleConfig {
  cron: string;
  timezone?: string;
}

export interface WebhookConfig {
  path: string;
  method: "GET" | "POST" | "PUT";
  secret?: string;
}

export interface FileWatchConfig {
  paths: string[];
  patterns?: string[];
  events: ("create" | "modify" | "delete")[];
}

export interface EventConfig {
  source: "agent_complete" | "document_save" | "pipeline_finish" | "error" | "custom";
  sourceId?: string;
  filter?: string;
}

export interface EmailConfig {
  mailbox: string;
  from_filter?: string;
  subject_filter?: string;
}

export interface GitConfig {
  repo?: string;
  events: ("push" | "pull_request" | "commit" | "tag" | "release")[];
  branch_filter?: string;
}

export interface QueueConfig {
  provider: "sqs" | "redis" | "kafka" | "rabbitmq";
  queue_name: string;
  connection_url?: string;
}

export interface ManualConfig {
  label?: string;
  confirmation?: boolean;
}

export interface RssConfig {
  feed_url: string;
  poll_interval?: number;  // minutes between checks (default 15)
  keyword_filter?: string; // only trigger on items matching keywords
}

export interface DatabaseConfig {
  connection: string;      // connection name or URI
  table: string;
  events: ("insert" | "update" | "delete")[];
  filter?: string;         // SQL WHERE clause filter
}

export interface ChatConfig {
  platform: "slack" | "teams" | "discord" | "custom";
  channel: string;
  keyword_filter?: string; // trigger only on messages matching keywords
  from_filter?: string;    // trigger only from specific users/bots
}

export interface CalendarConfig {
  calendar_id: string;
  trigger_when: "event_start" | "event_end" | "minutes_before" | "event_created";
  minutes_before?: number; // for "minutes_before" trigger
  event_filter?: string;   // filter by event title pattern
}

export interface ApiPollConfig {
  url: string;
  method: "GET" | "POST";
  headers?: Record<string, string>;
  poll_interval: number;   // seconds between polls
  change_detection: "any_change" | "json_path" | "status_code" | "hash";
  json_path?: string;      // JSONPath to monitor for changes
}

export interface ThresholdConfig {
  metric_source: "prometheus" | "cloudwatch" | "datadog" | "custom_api";
  metric_query: string;
  operator: "gt" | "lt" | "gte" | "lte" | "eq" | "neq";
  value: number;
  duration?: number;       // seconds the condition must hold before triggering
  cooldown?: number;       // seconds before re-triggering
}

export interface MqttConfig {
  broker_url: string;      // e.g. "mqtt://localhost:1883"
  topic: string;           // MQTT topic to subscribe to, supports wildcards (e.g. "sensors/+/temperature")
  qos?: 0 | 1 | 2;        // quality of service level
  username?: string;
  password?: string;
  payload_format?: "json" | "text" | "binary";
}

export interface TriggerConfig {
  type: TriggerType;
  schedule?: ScheduleConfig;
  webhook?: WebhookConfig;
  file_watch?: FileWatchConfig;
  event?: EventConfig;
  email?: EmailConfig;
  git?: GitConfig;
  queue?: QueueConfig;
  manual?: ManualConfig;
  rss?: RssConfig;
  database?: DatabaseConfig;
  chat?: ChatConfig;
  calendar?: CalendarConfig;
  api_poll?: ApiPollConfig;
  threshold?: ThresholdConfig;
  mqtt?: MqttConfig;
}

export interface TriggerDef {
  id: string;
  name: string;
  description: string;
  agentId: string;
  trigger: TriggerConfig;
  enabled: boolean;
  lastRun?: string;
  runCount: number;
}

/* ------------------------------------------------------------------ */
/*  Trigger type metadata                                             */
/* ------------------------------------------------------------------ */

export const TRIGGER_TYPES: {
  id: TriggerType;
  label: string;
  description: string;
  icon: string;
}[] = [
  { id: "schedule",   label: "Schedule (Cron)", description: "Run on a time-based schedule using cron expressions",            icon: "\u23F0" },
  { id: "webhook",    label: "Webhook",         description: "Triggered by an incoming HTTP request",                          icon: "\uD83C\uDF10" },
  { id: "file_watch", label: "File Watch",       description: "Watch files or directories for changes",                        icon: "\uD83D\uDCC2" },
  { id: "event",      label: "Event",            description: "React to internal platform events like agent completion",       icon: "\u26A1" },
  { id: "email",      label: "Email Inbox",      description: "Trigger when an email arrives in a monitored mailbox",          icon: "\uD83D\uDCE7" },
  { id: "git",        label: "Git Event",        description: "Respond to git repository events like pushes and pull requests", icon: "\uD83D\uDD00" },
  { id: "queue",      label: "Message Queue",    description: "Consume messages from an external message queue",               icon: "\uD83D\uDCE5" },
  { id: "manual",     label: "Manual Run",       description: "Run on demand with a single click",                             icon: "\u25B6\uFE0F" },
  { id: "rss",        label: "RSS / Feed",       description: "Trigger when new items appear in an RSS or Atom feed",          icon: "\uD83D\uDCE1" },
  { id: "database",   label: "Database",         description: "Trigger on database row insert, update, or delete",             icon: "\uD83D\uDDC3\uFE0F" },
  { id: "chat",       label: "Slack / Chat",     description: "Trigger when a message arrives in a chat channel",              icon: "\uD83D\uDCAC" },
  { id: "calendar",   label: "Calendar",         description: "Trigger before, during, or after calendar events",              icon: "\uD83D\uDCC5" },
  { id: "api_poll",   label: "API Poll",         description: "Periodically poll an API endpoint and trigger on changes",      icon: "\uD83D\uDD04" },
  { id: "threshold",  label: "Threshold / Alert", description: "Trigger when a metric crosses a threshold value",              icon: "\uD83D\uDCC8" },
  { id: "mqtt",       label: "MQTT",              description: "Subscribe to MQTT topics and trigger on incoming messages",    icon: "\uD83D\uDCE1" },
];

/* ------------------------------------------------------------------ */
/*  Built-in example triggers                                         */
/* ------------------------------------------------------------------ */

const builtInTriggers: TriggerDef[] = [
  {
    id: "trig-daily-standup",
    name: "Daily Standup Summary",
    description: "Generate a meeting summary every weekday at 9 AM Eastern",
    agentId: "meeting-minutes-creator",
    trigger: {
      type: "schedule",
      schedule: { cron: "0 9 * * MON-FRI", timezone: "America/New_York" },
    },
    enabled: true,
    lastRun: "2026-03-05T14:00:00Z",
    runCount: 84,
  },
  {
    id: "trig-weekly-dep-audit",
    name: "Weekly Dependency Audit",
    description: "Scan project dependencies for vulnerabilities every Monday at 6 AM",
    agentId: "dependency-vulnerability-scanner",
    trigger: {
      type: "schedule",
      schedule: { cron: "0 6 * * MON", timezone: "UTC" },
    },
    enabled: true,
    lastRun: "2026-03-02T06:00:00Z",
    runCount: 22,
  },
  {
    id: "trig-webhook-doc-ingest",
    name: "Incoming Document Processor",
    description: "Process documents submitted via webhook endpoint",
    agentId: "doc-template-generator",
    trigger: {
      type: "webhook",
      webhook: { path: "/hooks/doc-ingest", method: "POST", secret: "whsec_***" },
    },
    enabled: true,
    lastRun: "2026-03-05T18:32:00Z",
    runCount: 137,
  },
  {
    id: "trig-webhook-cicd",
    name: "CI/CD Pipeline Notification",
    description: "Receive deployment status from CI/CD pipeline",
    agentId: "deployment-pipeline-builder",
    trigger: {
      type: "webhook",
      webhook: { path: "/hooks/cicd-notify", method: "POST" },
    },
    enabled: true,
    lastRun: "2026-03-05T20:15:00Z",
    runCount: 56,
  },
  {
    id: "trig-file-inbox",
    name: "Document Inbox Watcher",
    description: "Watch inbox folder for new documents and route for processing",
    agentId: "layout-optimizer",
    trigger: {
      type: "file_watch",
      file_watch: {
        paths: ["/data/inbox", "/data/uploads"],
        patterns: ["*.md", "*.pdf", "*.docx"],
        events: ["create"],
      },
    },
    enabled: true,
    lastRun: "2026-03-05T16:45:00Z",
    runCount: 203,
  },
  {
    id: "trig-event-chain",
    name: "Post-Analysis Chain",
    description: "After static analysis completes, run the report builder",
    agentId: "report-skeleton-builder",
    trigger: {
      type: "event",
      event: {
        source: "agent_complete",
        sourceId: "static-analysis-runner",
      },
    },
    enabled: true,
    lastRun: "2026-03-05T12:10:00Z",
    runCount: 41,
  },
  {
    id: "trig-email-support",
    name: "Support Inbox Monitor",
    description: "Process incoming support emails and generate FAQ entries",
    agentId: "faq-page-creator",
    trigger: {
      type: "email",
      email: {
        mailbox: "support@celerex.io",
        subject_filter: "*help*|*question*|*issue*",
      },
    },
    enabled: true,
    lastRun: "2026-03-05T19:20:00Z",
    runCount: 68,
  },
  {
    id: "trig-git-pr-review",
    name: "PR Code Review",
    description: "Automatically review code when a pull request is opened",
    agentId: "code-review-assistant",
    trigger: {
      type: "git",
      git: {
        repo: "celerex/platform",
        events: ["pull_request"],
        branch_filter: "feature/*",
      },
    },
    enabled: true,
    lastRun: "2026-03-05T17:55:00Z",
    runCount: 112,
  },
  {
    id: "trig-git-deploy-main",
    name: "Deploy on Push to Main",
    description: "Trigger deployment pipeline when code is pushed to main",
    agentId: "deployment-strategy-advisor",
    trigger: {
      type: "git",
      git: {
        repo: "celerex/platform",
        events: ["push"],
        branch_filter: "main",
      },
    },
    enabled: true,
    lastRun: "2026-03-05T21:00:00Z",
    runCount: 34,
  },
  {
    id: "trig-queue-worker",
    name: "Task Queue Worker",
    description: "Process tasks from the Redis job queue",
    agentId: "background-job-scheduler",
    trigger: {
      type: "queue",
      queue: {
        provider: "redis",
        queue_name: "celerex:jobs",
        connection_url: "redis://localhost:6379",
      },
    },
    enabled: false,
    lastRun: "2026-03-04T10:30:00Z",
    runCount: 512,
  },
  {
    id: "trig-manual-report",
    name: "Generate Report",
    description: "Manually trigger report generation on demand",
    agentId: "report-skeleton-builder",
    trigger: {
      type: "manual",
      manual: { label: "Generate Report", confirmation: true },
    },
    enabled: true,
    runCount: 7,
  },
  {
    id: "trig-manual-healthcheck",
    name: "Infrastructure Health Check",
    description: "Run a comprehensive health check across all infrastructure",
    agentId: "health-check-designer",
    trigger: {
      type: "manual",
      manual: { label: "Run Health Check", confirmation: false },
    },
    enabled: true,
    lastRun: "2026-03-05T08:00:00Z",
    runCount: 15,
  },

  /* ---- RSS / Feed ---- */
  {
    id: "trig-rss-tech-news",
    name: "Tech News Digest",
    description: "Monitor Hacker News RSS feed and summarize top stories daily",
    agentId: "executive-summary-agent",
    trigger: {
      type: "rss",
      rss: {
        feed_url: "https://hnrss.org/frontpage",
        poll_interval: 60,
        keyword_filter: "AI|LLM|infrastructure",
      },
    },
    enabled: true,
    lastRun: "2026-03-05T22:00:00Z",
    runCount: 45,
  },

  /* ---- Database ---- */
  {
    id: "trig-db-new-tickets",
    name: "New Ticket Processor",
    description: "Trigger when a new support ticket is inserted into the database",
    agentId: "faq-page-creator",
    trigger: {
      type: "database",
      database: {
        connection: "postgres://localhost:5432/celerex",
        table: "support_tickets",
        events: ["insert"],
        filter: "status = 'new'",
      },
    },
    enabled: true,
    lastRun: "2026-03-05T20:30:00Z",
    runCount: 89,
  },

  /* ---- Chat / Slack ---- */
  {
    id: "trig-slack-deploy",
    name: "Slack Deploy Command",
    description: "Trigger deployment when someone posts 'deploy' in the #releases channel",
    agentId: "deployment-strategy-advisor",
    trigger: {
      type: "chat",
      chat: {
        platform: "slack",
        channel: "#releases",
        keyword_filter: "deploy|release|ship",
      },
    },
    enabled: true,
    lastRun: "2026-03-05T18:00:00Z",
    runCount: 23,
  },

  /* ---- Calendar ---- */
  {
    id: "trig-calendar-meeting-prep",
    name: "Meeting Prep Automation",
    description: "15 minutes before a meeting, generate an agenda and talking points",
    agentId: "meeting-minutes-creator",
    trigger: {
      type: "calendar",
      calendar: {
        calendar_id: "primary",
        trigger_when: "minutes_before",
        minutes_before: 15,
        event_filter: "*standup*|*review*|*planning*",
      },
    },
    enabled: true,
    lastRun: "2026-03-05T08:45:00Z",
    runCount: 62,
  },

  /* ---- API Poll ---- */
  {
    id: "trig-api-status-monitor",
    name: "External API Status Monitor",
    description: "Poll the status page API every 5 minutes and alert on degradation",
    agentId: "alert-routing-configurator",
    trigger: {
      type: "api_poll",
      api_poll: {
        url: "https://status.example.com/api/v1/status",
        method: "GET",
        poll_interval: 300,
        change_detection: "json_path",
        json_path: "$.status.indicator",
      },
    },
    enabled: true,
    lastRun: "2026-03-05T21:55:00Z",
    runCount: 1440,
  },

  /* ---- Threshold / Alert ---- */
  {
    id: "trig-threshold-cpu",
    name: "High CPU Alert",
    description: "Trigger infrastructure scaling advisor when CPU exceeds 85% for 5 minutes",
    agentId: "synthetic-monitor-creator",
    trigger: {
      type: "threshold",
      threshold: {
        metric_source: "prometheus",
        metric_query: "avg(rate(cpu_usage_seconds_total[5m])) * 100",
        operator: "gt",
        value: 85,
        duration: 300,
        cooldown: 600,
      },
    },
    enabled: true,
    lastRun: "2026-03-05T15:30:00Z",
    runCount: 8,
  },
  {
    id: "trig-threshold-error-rate",
    name: "Error Rate Spike",
    description: "Alert when 5xx error rate exceeds 5% of total requests",
    agentId: "log-pattern-analyzer",
    trigger: {
      type: "threshold",
      threshold: {
        metric_source: "datadog",
        metric_query: "sum:http.5xx_errors / sum:http.total_requests * 100",
        operator: "gt",
        value: 5,
        duration: 120,
        cooldown: 300,
      },
    },
    enabled: true,
    lastRun: "2026-03-05T14:20:00Z",
    runCount: 3,
  },

  /* ---- MQTT ---- */
  {
    id: "trig-mqtt-sensor-data",
    name: "IoT Sensor Data Processor",
    description: "Subscribe to MQTT temperature sensor topic and trigger analysis when data arrives",
    agentId: "data-quality-monitor",
    trigger: {
      type: "mqtt",
      mqtt: {
        broker_url: "mqtt://iot-broker.local:1883",
        topic: "sensors/+/temperature",
        qos: 1,
        payload_format: "json",
      },
    },
    enabled: true,
    lastRun: "2026-03-05T21:58:00Z",
    runCount: 2847,
  },
  {
    id: "trig-mqtt-factory-alerts",
    name: "Factory Floor Alerts",
    description: "Monitor MQTT alerts from factory equipment and route to maintenance agents",
    agentId: "alert-routing-configurator",
    trigger: {
      type: "mqtt",
      mqtt: {
        broker_url: "mqtt://factory.internal:1883",
        topic: "factory/+/alerts/#",
        qos: 2,
        payload_format: "json",
      },
    },
    enabled: false,
    lastRun: "2026-03-04T16:00:00Z",
    runCount: 156,
  },
];

/* ------------------------------------------------------------------ */
/*  Persistence (localStorage)                                        */
/* ------------------------------------------------------------------ */

const STORAGE_KEY = "celerex-triggers";

function loadTriggers(): TriggerDef[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function saveTriggers(triggers: TriggerDef[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(triggers));
}

let customTriggers: TriggerDef[] = loadTriggers();

/* ------------------------------------------------------------------ */
/*  CRUD API                                                          */
/* ------------------------------------------------------------------ */

export function getAllTriggers(): TriggerDef[] {
  return [...builtInTriggers, ...customTriggers];
}

export function addTrigger(trigger: TriggerDef) {
  customTriggers = [...customTriggers, trigger];
  saveTriggers(customTriggers);
}

export function updateTrigger(id: string, updates: Partial<TriggerDef>) {
  // Check built-in first — if it's built-in, copy to custom with overrides
  const builtInIdx = builtInTriggers.findIndex((t) => t.id === id);
  if (builtInIdx !== -1) {
    const existing = customTriggers.find((t) => t.id === id);
    if (existing) {
      customTriggers = customTriggers.map((t) =>
        t.id === id ? { ...t, ...updates } : t
      );
    } else {
      customTriggers = [...customTriggers, { ...builtInTriggers[builtInIdx], ...updates }];
    }
  } else {
    customTriggers = customTriggers.map((t) =>
      t.id === id ? { ...t, ...updates } : t
    );
  }
  saveTriggers(customTriggers);
}

export function deleteTrigger(id: string) {
  customTriggers = customTriggers.filter((t) => t.id !== id);
  saveTriggers(customTriggers);
}

export function toggleTrigger(id: string) {
  const all = getAllTriggers();
  const trigger = all.find((t) => t.id === id);
  if (trigger) {
    updateTrigger(id, { enabled: !trigger.enabled });
  }
}
