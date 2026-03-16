import type {
  BacktestRequest,
  BacktestResponse,
  CheckRequest,
  CheckResponse,
  CompletionsRequest,
  CompletionsResponse,
  HoverRequest,
  HoverResponse,
  PaperDashboardOverview,
  PaperSessionDetailResponse,
  PaperSessionLogsResponse,
  PublicDatasetCatalog,
} from "./types";

const SESSION_KEY = "palmscript.ide.session";

function browserSessionId(): string {
  const existing = window.localStorage.getItem(SESSION_KEY);
  if (existing) {
    return existing;
  }
  const session = `web-${crypto.randomUUID()}`;
  window.localStorage.setItem(SESSION_KEY, session);
  return session;
}

async function parseJson<T>(response: Response): Promise<T> {
  if (response.ok) {
    return (await response.json()) as T;
  }

  let message = `${response.status} ${response.statusText}`;
  try {
    const payload = (await response.json()) as { error?: string };
    if (payload.error) {
      message = payload.error;
    }
  } catch {
    // Fall back to the HTTP status text.
  }
  throw new Error(message);
}

async function requestJson<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: {
      "content-type": "application/json",
      "x-palmscript-session": browserSessionId(),
      ...(init?.headers ?? {}),
    },
  });
  return parseJson<T>(response);
}

export function fetchDatasets(): Promise<PublicDatasetCatalog> {
  return requestJson<PublicDatasetCatalog>("./api/datasets");
}

export function checkScript(request: CheckRequest): Promise<CheckResponse> {
  return requestJson<CheckResponse>("./api/check", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export function fetchHover(request: HoverRequest): Promise<HoverResponse> {
  return requestJson<HoverResponse>("./api/hover", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export function fetchCompletions(
  request: CompletionsRequest,
): Promise<CompletionsResponse> {
  return requestJson<CompletionsResponse>("./api/completions", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export function runBacktest(
  request: BacktestRequest,
): Promise<BacktestResponse> {
  return requestJson<BacktestResponse>("./api/backtest", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export function fetchPaperOverview(): Promise<PaperDashboardOverview> {
  return requestJson<PaperDashboardOverview>("./api/paper/overview");
}

export function fetchPaperSessionDetail(
  sessionId: string,
): Promise<PaperSessionDetailResponse> {
  return requestJson<PaperSessionDetailResponse>(
    `./api/paper/sessions/${encodeURIComponent(sessionId)}`,
  );
}

export function fetchPaperSessionLogs(
  sessionId: string,
): Promise<PaperSessionLogsResponse> {
  return requestJson<PaperSessionLogsResponse>(
    `./api/paper/sessions/${encodeURIComponent(sessionId)}/logs`,
  );
}
