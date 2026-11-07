import { useState } from "react";
import { executeAction } from "@/platform/actions";
import { useTx } from "@/platform/i18n";
import { useProject } from "@/platform/project";
import { editorService, useContributions } from "@/workbench/contributions";
import { endpoints, visibleRows, type Endpoint, type RuntimeRow, type RuntimeState } from "./model";
import { connect, disconnect, refresh, useRuntime } from "./store";
import { sourceViewId } from "./sourceViews";
import { sourceTarget } from "./navigation";
import styles from "./RuntimePanel.module.css";

const states: Record<RuntimeState, string> = {
  ok: "Available",
  unauthorized: "Authentication required (401)",
  forbidden: "Permission denied (403)",
  disabled: "Endpoint unavailable (404): not exposed, disabled, or wrong base path",
  redirect: "Redirect refused: use the explicit loopback endpoint",
  httpError: "Endpoint returned an HTTP error",
  timeout: "Runtime request timed out",
  unreachable: "No reachable local runtime at this address",
  tooLarge: "Endpoint response exceeds the size limit",
  invalidResponse: "Runtime response could not be read",
  incompatible: "Unsupported Actuator response structure or version",
  cancelled: "Runtime request cancelled",
};
const labels: Record<Endpoint, string> = {
  beans: "Beans",
  conditions: "Conditions",
  env: "Environment keys",
  configprops: "Configuration properties",
  mappings: "Mappings",
};

export function RuntimePanel() {
  const root = useProject((s) => s.project?.root);
  return <RuntimeConnection key={root ?? ""} root={root} />;
}

function RuntimeConnection({ root }: { root: string | undefined }) {
  const tx = useTx();
  useContributions((s) => s.toolWindows);
  const { phase, snapshot } = useRuntime();
  const [base, setBase] = useState("http://localhost:8080/actuator");
  const [user, setUser] = useState("");
  const [password, setPassword] = useState("");
  const [endpoint, setEndpoint] = useState<Endpoint>("beans");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const [navigation, setNavigation] = useState("");

  const data = snapshot?.[endpoint];
  const rows = visibleRows(data, query);
  const last = Math.max(0, Math.ceil(rows.length / 100) - 1);
  const current = Math.min(page, last);
  const busy = phase === "connecting";
  async function open(row: RuntimeRow) {
    if (!root) return;
    setNavigation(tx("Resolving runtime source…"));
    try {
      const target = await sourceTarget(row, root);
      if (useProject.getState().project?.root !== root || useRuntime.getState().snapshot !== snapshot) return;
      if (!target) {
        setNavigation(tx("Source identity is unresolved or ambiguous"));
        return;
      }
      await editorService().openFile(target.path, { selection: target.selection, preview: true });
      setNavigation("");
    } catch {
      setNavigation(tx("Source identity is unresolved or ambiguous"));
    }
  }
  return (
    <section className={styles.panel} aria-label={tx("Spring Runtime")}>
      <form
        className={styles.toolbar}
        onSubmit={(e) => {
          e.preventDefault();
          if (!root || busy) return;
          void connect(base.trim(), user, password);
          setPassword("");
          setNavigation("");
        }}
      >
        <input
          aria-label={tx("Loopback Actuator base URL")}
          value={base}
          onChange={(e) => setBase(e.target.value)}
          spellCheck={false}
        />
        <input
          aria-label={tx("Runtime username (optional)")}
          placeholder={tx("Username (optional)")}
          value={user}
          onChange={(e) => setUser(e.target.value)}
          autoComplete="off"
        />
        <input
          aria-label={tx("Runtime password (memory only)")}
          placeholder={tx("Password (memory only)")}
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="off"
        />
        <button type="submit" disabled={!root || busy}>
          {tx("Connect")}
        </button>
        <button
          type="button"
          onClick={() => {
            void refresh();
            setNavigation("");
          }}
          disabled={busy || phase === "disconnected"}
        >
          {tx("Refresh")}
        </button>
        <button
          type="button"
          onClick={() => {
            disconnect();
            setUser("");
            setPassword("");
            setNavigation("");
          }}
          disabled={phase === "disconnected"}
        >
          {tx("Disconnect")}
        </button>
      </form>
      <p className={styles.notice}>
        {tx(
          "Read-only loopback connection. Configuration values are hidden; credentials stay in memory until disconnect. Enable only the endpoints you need in your development app.",
        )}
      </p>
      <div className={styles.toolbar}>
        <button
          type="button"
          disabled={!sourceViewId("beans")}
          onClick={() => {
            void executeAction("springRuntime.sourceBeans");
          }}
        >
          {tx("Spring Beans (source)")}
        </button>
        <button
          type="button"
          disabled={!sourceViewId("bean-graph")}
          onClick={() => {
            void executeAction("springRuntime.sourceGraph");
          }}
        >
          {tx("Spring Bean Graph (source)")}
        </button>
      </div>
      <div className={styles.toolbar}>
        <select
          aria-label={tx("Runtime endpoint")}
          value={endpoint}
          onChange={(e) => {
            setEndpoint(e.target.value as Endpoint);
            setPage(0);
            setNavigation("");
          }}
        >
          {endpoints.map((id) => (
            <option key={id} value={id}>
              {tx(labels[id])}
            </option>
          ))}
        </select>
        <input
          aria-label={tx("Filter runtime facts")}
          placeholder={tx("Filter runtime facts")}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setPage(0);
          }}
        />
      </div>
      <div role="status" className={styles.notice}>
        {!root
          ? tx("Open a Java project to connect")
          : phase === "disconnected"
            ? tx("Disconnected: connect explicitly to a local development app")
            : busy
              ? tx("Reading local runtime…")
              : phase === "error"
                ? tx("Connection rejected or unavailable. Check the loopback URL and retry.")
                : data
                  ? tx(states[data.state])
                  : ""}
        {data?.state === "ok" && (
          <>
            {" "}
            · {rows.length} {tx("runtime facts")}
            {data.limited ? ` · ${tx("Result limit reached")}` : ""}
          </>
        )}
        {navigation && <p>{navigation}</p>}
      </div>
      {data?.state === "ok" && rows.length === 0 && (
        <p className={styles.notice}>{tx("No runtime facts match this filter")}</p>
      )}
      <div className={styles.results}>
        {rows.slice(current * 100, (current + 1) * 100).map((row, i) => (
          <article key={`${endpoint}:${current}:${i}`} className={styles.row}>
            <div>
              <strong>{row.name.trim() || tx("Unknown runtime entry")}</strong>{" "}
              {row.masked && <span>{tx("Value hidden")}</span>}
            </div>
            <div className={styles.metadata}>
              {[
                row.context,
                row.detail,
                row.className,
                row.methodName,
                row.outcome
                  ? tx(
                      row.outcome === "matched"
                        ? "Matched"
                        : row.outcome === "notMatched"
                          ? "Not matched"
                          : "Unconditional",
                    )
                  : "",
              ]
                .filter(Boolean)
                .join(" · ")}
            </div>
            {!!row.dependencies?.length && (
              <div>
                {tx("Runtime dependencies")}: {row.dependencies.join(", ")}
              </div>
            )}
            {row.className && (
              <button
                type="button"
                onClick={() => {
                  void open(row);
                }}
              >
                {tx(row.methodName ? "Locate mapping source" : "Locate runtime type source")}
              </button>
            )}
          </article>
        ))}
      </div>
      {last > 0 && (
        <nav className={styles.toolbar} aria-label={tx("Runtime result pages")}>
          <button disabled={current === 0} onClick={() => setPage(current - 1)}>
            {tx("Previous")}
          </button>
          <span>
            {current + 1} / {last + 1}
          </span>
          <button disabled={current === last} onClick={() => setPage(current + 1)}>
            {tx("Next")}
          </button>
        </nav>
      )}
    </section>
  );
}
