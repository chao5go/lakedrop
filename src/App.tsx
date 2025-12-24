import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import Editor from "@monaco-editor/react";
import { useVirtualizer } from "@tanstack/react-virtual";
import clsx from "clsx";
import toast, { Toaster } from "react-hot-toast";
import "./App.css";

type FieldInfo = {
  name: string;
  dtype: string;
};

type FileMetadataResponse = {
  file_name: string;
  file_path: string;
  file_size: number;
  row_count: number;
  schema: FieldInfo[];
  sheets: string[];
  active_sheet?: string | null;
};

type ColumnInfo = {
  name: string;
  dtype: string;
};

type QueryResult = {
  columns: ColumnInfo[];
  rows: unknown[][];
  row_count: number;
};

type ContextMenuState = {
  x: number;
  y: number;
  value: string;
  row: unknown[];
};

const DEFAULT_SQL = "SELECT * FROM source LIMIT 3";

function formatBytes(bytes: number) {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let index = 0;
  let value = bytes;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value.toFixed(value >= 10 ? 1 : 2)} ${units[index]}`;
}

function compareValues(a: unknown, b: unknown) {
  if (a === null || a === undefined) return b === null || b === undefined ? 0 : 1;
  if (b === null || b === undefined) return -1;
  if (typeof a === "number" && typeof b === "number") return a - b;
  return String(a).localeCompare(String(b), undefined, { numeric: true });
}

function App() {
  const { t, i18n } = useTranslation();
  const [fileMeta, setFileMeta] = useState<FileMetadataResponse | null>(null);
  const [sql, setSql] = useState(DEFAULT_SQL);
  const [result, setResult] = useState<QueryResult | null>(null);
  const [isDragOver, setIsDragOver] = useState(false);
  const [isLoadingFile, setIsLoadingFile] = useState(false);
  const [isRunningQuery, setIsRunningQuery] = useState(false);
  const [queryMs, setQueryMs] = useState<number | null>(null);
  const [theme, setTheme] = useState<"light" | "dark">("light");
  const [menuState, setMenuState] = useState<ContextMenuState | null>(null);
  const tableParentRef = useRef<HTMLDivElement>(null);
  const resizingRef = useRef<{
    index: number;
    startX: number;
    startWidth: number;
  } | null>(null);
  const [columnWidths, setColumnWidths] = useState<number[]>([]);
  const [sortState, setSortState] = useState<{
    index: number;
    direction: "asc" | "desc";
  } | null>(null);
  const columns = result?.columns ?? [];
  const rows = result?.rows ?? [];
  const displayRows = useMemo(() => {
    if (!sortState) return rows;
    const sorted = [...rows];
    sorted.sort((a, b) =>
      compareValues(a[sortState.index], b[sortState.index]),
    );
    if (sortState.direction === "desc") {
      sorted.reverse();
    }
    return sorted;
  }, [rows, sortState]);
  const gridTemplate = useMemo(() => {
    if (!columns.length) return "1fr";
    if (columnWidths.length === columns.length) {
      return columnWidths.map((width) => `${width}px`).join(" ");
    }
    return `repeat(${columns.length}, minmax(140px, 1fr))`;
  }, [columnWidths, columns.length]);

  const rowVirtualizer = useVirtualizer({
    count: displayRows.length,
    getScrollElement: () => tableParentRef.current,
    estimateSize: () => 34,
    overscan: 12,
  });

  const virtualRows = rowVirtualizer.getVirtualItems();
  const totalSize = rowVirtualizer.getTotalSize();

  useEffect(() => {
    const storedTheme = window.localStorage.getItem("lakedrop-theme");
    const storedLanguage = window.localStorage.getItem("lakedrop-lang");
    if (storedTheme === "dark" || storedTheme === "light") {
      setTheme(storedTheme);
    } else if (window.matchMedia?.("(prefers-color-scheme: dark)").matches) {
      setTheme("dark");
    }
    if (storedLanguage) {
      i18n.changeLanguage(storedLanguage).catch(() => undefined);
    }
  }, [i18n]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    window.localStorage.setItem("lakedrop-theme", theme);
  }, [theme]);

  useEffect(() => {
    let unlistenDrop: (() => void) | null = null;
    let unlistenHover: (() => void) | null = null;
    let unlistenCancel: (() => void) | null = null;

    listen<string[]>("tauri://file-drop", (event) => {
      const [path] = event.payload ?? [];
      setIsDragOver(false);
      if (path) {
        loadFile(path);
      }
    }).then((unlisten) => {
      unlistenDrop = unlisten;
    });

    listen("tauri://file-drop-hover", () => setIsDragOver(true)).then(
      (unlisten) => {
        unlistenHover = unlisten;
      },
    );

    listen("tauri://file-drop-cancelled", () => setIsDragOver(false)).then(
      (unlisten) => {
        unlistenCancel = unlisten;
      },
    );

    return () => {
      unlistenDrop?.();
      unlistenHover?.();
      unlistenCancel?.();
    };
  }, []);

  useEffect(() => {
    const closeMenu = () => setMenuState(null);
    window.addEventListener("click", closeMenu);
    return () => window.removeEventListener("click", closeMenu);
  }, []);

  useEffect(() => {
    if (!columns.length) {
      setColumnWidths([]);
      return;
    }
    setColumnWidths((prev) =>
      prev.length === columns.length ? prev : columns.map(() => 180),
    );
  }, [columns.length]);

  useEffect(() => {
    const handleMove = (event: MouseEvent) => {
      if (!resizingRef.current) return;
      const { index, startX, startWidth } = resizingRef.current;
      const nextWidth = Math.max(120, startWidth + event.clientX - startX);
      setColumnWidths((prev) => {
        const next = [...prev];
        next[index] = nextWidth;
        return next;
      });
    };

    const handleUp = () => {
      resizingRef.current = null;
    };

    window.addEventListener("mousemove", handleMove);
    window.addEventListener("mouseup", handleUp);
    return () => {
      window.removeEventListener("mousemove", handleMove);
      window.removeEventListener("mouseup", handleUp);
    };
  }, []);

  async function loadFile(path: string) {
    setIsLoadingFile(true);
    try {
      const response = await invoke<FileMetadataResponse>("scan_file_metadata", {
        path,
      });
      setFileMeta(response);
      setSql(DEFAULT_SQL);
      setResult(null);
      setQueryMs(null);
      setSortState(null);
      toast.success(t("fileLoaded"));
      await runQuery(DEFAULT_SQL);
    } catch (error) {
      toast.error(String(error));
    } finally {
      setIsLoadingFile(false);
    }
  }

  async function runQuery(overrideSql?: string) {
    const queryText = overrideSql ?? sql;
    if (!queryText.trim()) {
      toast.error(t("sqlEmpty"));
      return;
    }
    setIsRunningQuery(true);
    const start = performance.now();
    try {
      const response = await invoke<QueryResult>("exec_sql", {
        sql: queryText,
        maxRows: 1000,
      });
      setResult(response);
      setQueryMs(Math.round(performance.now() - start));
      setSortState(null);
    } catch (error) {
      toast.error(String(error));
    } finally {
      setIsRunningQuery(false);
    }
  }

  async function exportQuery(format: "csv" | "xlsx") {
    if (!fileMeta) {
      toast.error(t("noFile"));
      return;
    }
    const defaultName = fileMeta.file_name.replace(/\.[^.]+$/, "");
    const path = await save({
      title: t("exportTitle"),
      defaultPath: `${defaultName}.${format}`,
      filters: [{ name: format.toUpperCase(), extensions: [format] }],
    });
    if (!path) return;
    try {
      await invoke("export_query", {
        sql,
        path,
        format,
      });
      toast.success(t("exportSuccess"));
    } catch (error) {
      toast.error(String(error));
    }
  }

  async function selectSheet(sheet: string) {
    if (!sheet) return;
    try {
      const response = await invoke<FileMetadataResponse>("select_excel_sheet", {
        sheet,
      });
      setFileMeta(response);
      setResult(null);
      setQueryMs(null);
      setSortState(null);
    } catch (error) {
      toast.error(String(error));
    }
  }

  async function pickFile() {
    const path = await open({
      multiple: false,
      filters: [
        {
          name: "Data Files",
          extensions: [
            "parquet",
            "parq",
            "csv",
            "tsv",
            "txt",
            "jsonl",
            "ndjson",
            "json",
            "arrow",
            "feather",
            "ipc",
            "gz",
          ],
        },
      ],
    });
    if (typeof path === "string") {
      loadFile(path);
    }
  }

  async function loadSample(fileName: string) {
    try {
      const path = await invoke<string>("resolve_sample_path", {
        fileName,
      });
      await loadFile(path);
    } catch (error) {
      toast.error(String(error));
    }
  }

  return (
    <div className="app">
      <Toaster position="top-right" />
      <header className="header">
        <div>
          <p className="eyebrow">LakeDrop</p>
          <h1>{t("title")}</h1>
          <p className="subtitle">{t("subtitle")}</p>
        </div>
        <div className="header-actions">
          <button
            className="ghost-button"
            onClick={() =>
              setTheme((prev) => (prev === "dark" ? "light" : "dark"))
            }
          >
            {theme === "dark" ? t("themeLight") : t("themeDark")}
          </button>
          <button
            className="ghost-button"
            onClick={() => {
              const next = i18n.language.startsWith("zh") ? "en" : "zh";
              i18n.changeLanguage(next).catch(() => undefined);
              window.localStorage.setItem("lakedrop-lang", next);
            }}
          >
            {i18n.language.startsWith("zh") ? "EN" : "中文"}
          </button>
        </div>
      </header>

      <main className="main">
        <section className="sidebar">
          <div className="panel">
            <div className="panel-header">
              <h2>{t("fileInfo")}</h2>
              <button className="ghost-button" onClick={pickFile}>
                {t("openFile")}
              </button>
            </div>
            {fileMeta ? (
              <div className="meta-grid">
                <div>
                  <span>{t("fileName")}</span>
                  <strong>{fileMeta.file_name}</strong>
                </div>
                <div>
                  <span>{t("fileSize")}</span>
                  <strong>{formatBytes(fileMeta.file_size)}</strong>
                </div>
                <div>
                  <span>{t("rowCount")}</span>
                  <strong>{fileMeta.row_count.toLocaleString()}</strong>
                </div>
                <div>
                  <span>{t("filePath")}</span>
                  <strong title={fileMeta.file_path}>{fileMeta.file_path}</strong>
                </div>
                {fileMeta.sheets.length > 0 && (
                  <div className="sheet-row">
                    <span>{t("sheet")}</span>
                    <select
                      value={fileMeta.active_sheet ?? fileMeta.sheets[0]}
                      onChange={(event) => selectSheet(event.target.value)}
                    >
                      {fileMeta.sheets.map((sheet) => (
                        <option key={sheet} value={sheet}>
                          {sheet}
                        </option>
                      ))}
                    </select>
                  </div>
                )}
              </div>
            ) : (
              <p className="muted">{t("noFile")}</p>
            )}
          </div>

          <div className="panel">
            <h2>{t("samples")}</h2>
            <div className="sample-grid">
              <button onClick={() => loadSample("sample.csv")}>CSV</button>
              <button onClick={() => loadSample("sample.jsonl")}>JSONL</button>
              <button onClick={() => loadSample("sample.parquet")}>Parquet</button>
              <button onClick={() => loadSample("sample.arrow")}>Arrow</button>
              <button onClick={() => loadSample("sample.xlsx")}>Excel</button>
            </div>
            <p className="muted">{t("samplesHint")}</p>
          </div>

          <div className="panel">
            <h2>{t("schema")}</h2>
            {fileMeta ? (
              <div className="schema-list">
                {fileMeta.schema.map((field) => (
                  <div key={field.name} className="schema-row">
                    <span>{field.name}</span>
                    <em>{field.dtype}</em>
                  </div>
                ))}
              </div>
            ) : (
              <p className="muted">{t("schemaHint")}</p>
            )}
          </div>
        </section>

        <section className="workspace">
          <div className="panel editor-panel">
            <div className="panel-header">
              <h2>{t("sqlEditor")}</h2>
              <div className="panel-actions">
                <button
                  className="primary-button"
                  onClick={() => runQuery()}
                  disabled={isRunningQuery}
                >
                  {isRunningQuery ? t("running") : t("runQuery")}
                </button>
                <button
                  className="ghost-button"
                  onClick={() => exportQuery("csv")}
                  disabled={!fileMeta}
                >
                  {t("exportCsv")}
                </button>
                <button
                  className="ghost-button"
                  onClick={() => exportQuery("xlsx")}
                  disabled={!fileMeta}
                >
                  {t("exportXlsx")}
                </button>
              </div>
            </div>
            <div className="editor-shell">
              <Editor
                height="200px"
                language="sql"
                theme={theme === "dark" ? "vs-dark" : "light"}
                value={sql}
                onChange={(value) => setSql(value ?? "")}
                options={{
                  fontSize: 14,
                  minimap: { enabled: false },
                  scrollBeyondLastLine: false,
                }}
                onMount={(editor, monaco) => {
                  editor.addAction({
                    id: "run-query",
                    label: "Run Query",
                    keybindings: [
                      monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter,
                    ],
                    run: () => runQuery(),
                  });
                }}
              />
            </div>
          </div>

          <div className="panel table-panel">
            <div className="panel-header">
              <h2>{t("results")}</h2>
              <div className="meta-line">
                <span>
                  {result
                    ? t("rowsShown", { count: displayRows.length })
                    : t("noResults")}
                </span>
                <span>
                  {queryMs !== null
                    ? t("queryTime", { ms: queryMs })
                    : ""}
                </span>
              </div>
            </div>
            <div className="table" ref={tableParentRef}>
              {columns.length === 0 ? (
                <div className="empty-state">
                  <p>{t("dropHint")}</p>
                  <p className="muted">{t("dropHintSub")}</p>
                </div>
              ) : (
                <div className="table-grid">
                  <div
                    className="table-header"
                    style={{ gridTemplateColumns: gridTemplate }}
                  >
                    {columns.map((column, index) => {
                      const isSorted = sortState?.index === index;
                      return (
                        <div
                          key={column.name}
                          className={clsx("header-cell", {
                            "is-sorted": isSorted,
                          })}
                          onClick={() => {
                            if (!sortState || sortState.index !== index) {
                              setSortState({ index, direction: "asc" });
                            } else if (sortState.direction === "asc") {
                              setSortState({ index, direction: "desc" });
                            } else {
                              setSortState(null);
                            }
                          }}
                        >
                          <div className="header-title">
                            <strong>{column.name}</strong>
                            <span className="sort-indicator">
                              {isSorted
                                ? sortState?.direction === "asc"
                                  ? "▲"
                                  : "▼"
                                : ""}
                            </span>
                          </div>
                          <span>{column.dtype}</span>
                          <span
                            className="resize-handle"
                            onMouseDown={(event) => {
                              event.stopPropagation();
                              resizingRef.current = {
                                index,
                                startX: event.clientX,
                                startWidth: columnWidths[index] ?? 180,
                              };
                            }}
                          />
                        </div>
                      );
                    })}
                  </div>
                  <div
                    className="table-body"
                    style={{ height: totalSize }}
                  >
                    {virtualRows.map((virtualRow) => {
                      const row = displayRows[virtualRow.index] ?? [];
                      return (
                        <div
                          key={virtualRow.key}
                          className={clsx("table-row", {
                            "is-even": virtualRow.index % 2 === 0,
                          })}
                          style={{
                            transform: `translateY(${virtualRow.start}px)`,
                            gridTemplateColumns: gridTemplate,
                          }}
                        >
                          {row.map((cell, cellIndex) => {
                            const value = cell ?? "";
                            const display =
                              typeof value === "string"
                                ? value
                                : JSON.stringify(value);
                            return (
                              <div
                                key={`${virtualRow.key}-${cellIndex}`}
                                className="cell"
                                title={display}
                                onDoubleClick={() => {
                                  navigator.clipboard
                                    .writeText(display)
                                    .then(() =>
                                      toast.success(t("copied")),
                                    )
                                    .catch(() => toast.error(t("copyFailed")));
                                }}
                                onContextMenu={(event) => {
                                  event.preventDefault();
                                  setMenuState({
                                    x: event.clientX,
                                    y: event.clientY,
                                    value: display,
                                    row,
                                  });
                                }}
                              >
                                {display}
                              </div>
                            );
                          })}
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}
            </div>
            <div className="status-bar">
              <span>
                {fileMeta
                  ? `${formatBytes(fileMeta.file_size)} · ${fileMeta.row_count.toLocaleString()} ${t(
                      "rows",
                    )}`
                  : t("waiting")}
              </span>
              <span>
                {result ? t("loadedRows", { count: displayRows.length }) : ""}
              </span>
            </div>
          </div>
        </section>
      </main>

      {isDragOver && (
        <div className="drop-overlay">
          <div className="drop-card">
            <p>{t("dropOverlay")}</p>
            <span>{t("dropOverlayHint")}</span>
          </div>
        </div>
      )}

      {isLoadingFile && (
        <div className="loading-overlay">
          <div className="loading-card">
            <span className="spinner" />
            <p>{t("loading")}</p>
          </div>
        </div>
      )}

      {menuState && (
        <div
          className="context-menu"
          style={{ top: menuState.y, left: menuState.x }}
        >
          <button
            onClick={() => {
              navigator.clipboard
                .writeText(menuState.value)
                .then(() => toast.success(t("copied")))
                .catch(() => toast.error(t("copyFailed")));
              setMenuState(null);
            }}
          >
            {t("copyValue")}
          </button>
          <button
            onClick={() => {
              navigator.clipboard
                .writeText(JSON.stringify(menuState.row, null, 2))
                .then(() => toast.success(t("copied")))
                .catch(() => toast.error(t("copyFailed")));
              setMenuState(null);
            }}
          >
            {t("copyRow")}
          </button>
        </div>
      )}
    </div>
  );
}

export default App;
