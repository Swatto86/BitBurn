import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Window } from "@tauri-apps/api/window";
import { Event } from "@tauri-apps/api/event";

// Define extended DragDropEvent type
interface ExtendedDragDropEvent {
  type: string;
  paths?: string[];
  position?: { x: number; y: number };
}

interface WipeProgress {
  current_pass: number;
  total_passes: number;
  bytes_processed: number;
  total_bytes: number;
  current_algorithm: string;
  current_pattern: string;
  percentage: number;
  estimated_total_bytes?: number;
}

interface ContextWipePayload {
  paths: string[];
  invalid: string[];
  source: string;
}

interface ContextMenuStatus {
  enabled: boolean;
  message: string;
}

const MAX_FILE_SIZE = 1024 * 1024 * 1024 * 10; // 10GB warning threshold

function App() {
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [passes, setPasses] = useState<number>(3);
  const [algorithm, setAlgorithm] = useState<
    "NistClear" | "NistPurge" | "Gutmann" | "Random"
  >("NistPurge");
  const [isWiping, setIsWiping] = useState(false);
  const [result, setResult] = useState<{
    success: boolean;
    message: string;
  } | null>(null);
  const [theme, setTheme] = useState(() => {
    return localStorage.getItem("theme") || "dark";
  });
  const [isDragging, setIsDragging] = useState(false);
  const [wipeProgress, setWipeProgress] = useState<WipeProgress | null>(null);
  const [operationMode, setOperationMode] = useState<
    "initial" | "files" | "freespace"
  >("initial");
  const [abortController, setAbortController] =
    useState<AbortController | null>(null);
  const [isContextMode, setIsContextMode] = useState(false);
  const [contextInvalidPaths, setContextInvalidPaths] = useState<string[]>([]);
  const [isWindows, setIsWindows] = useState(false);
  const [contextMenuEnabled, setContextMenuEnabled] =
    useState<boolean | null>(null);
  const [contextMenuMessage, setContextMenuMessage] =
    useState<string | null>(null);
  const [contextMenuBusy, setContextMenuBusy] = useState(false);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
  }, []);

  useEffect(() => {
    const detectPlatform = async () => {
      const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
      const hasTauriIpc =
        typeof window !== "undefined" && "__TAURI_IPC__" in window;

      if (hasTauriIpc) {
        try {
          const info = (await invoke("platform_info")) as {
            is_windows?: boolean;
          };
          if (typeof info?.is_windows === "boolean") {
            setIsWindows(info.is_windows);
            return;
          }
        } catch (error) {
          console.error("Unable to detect platform:", error);
        }
      }

      setIsWindows(ua.toLowerCase().includes("windows"));
    };

    detectPlatform();
  }, []);

  useEffect(() => {
    if (!isWindows) return;
    refreshContextMenuStatus();
  }, [isWindows]);

  const toggleTheme = () => {
    const newTheme = theme === "dark" ? "light" : "dark";
    setTheme(newTheme);
    localStorage.setItem("theme", newTheme);
    document.documentElement.setAttribute("data-theme", newTheme);
  };

  useEffect(() => {
    let unlistenDragDrop: (() => void) | undefined;

    const setupDragDrop = async () => {
      try {
        const window = new Window("main");
        unlistenDragDrop = await window.onDragDropEvent(
          (event: Event<ExtendedDragDropEvent>) => {
            if (
              event.payload.type === "drop" &&
              event.payload.paths &&
              !isWiping
            ) {
              setSelectedPaths((prev) => [
                ...new Set([...prev, ...event.payload.paths!]),
              ]);
              setIsDragging(false);
              setOperationMode("files");
            } else if (event.payload.type === "enter") {
              setIsDragging(true);
            } else if (event.payload.type === "leave") {
              setIsDragging(false);
            }
          },
        );
      } catch (error) {
        console.error("Error setting up drag and drop:", error);
      }
    };

    setupDragDrop();

    return () => {
      if (unlistenDragDrop) {
        unlistenDragDrop();
      }
    };
  }, [isWiping]);

  useEffect(() => {
    switch (algorithm) {
      case "NistClear":
        setPasses(1);
        break;
      case "NistPurge":
        setPasses(3);
        break;
      case "Gutmann":
        setPasses(35);
        break;
      // For Random, keep the user-selected value
    }
  }, [algorithm]);

  useEffect(() => {
    let unlistenFn: (() => void) | undefined;

    async function setupListener() {
      const window = new Window("main");
      unlistenFn = await window.listen<WipeProgress>(
        "wipe_progress",
        (event: Event<WipeProgress>) => {
          setWipeProgress(event.payload);
        },
      );
    }

    setupListener();

    return () => {
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, []);

  useEffect(() => {
    let unlistenContext: (() => void) | undefined;

    const setupContextListener = async () => {
      try {
        const window = new Window("main");
        unlistenContext = await window.listen<ContextWipePayload>(
          "context_wipe_request",
          (event: Event<ContextWipePayload>) => {
            const payload = event.payload;
            const unique = Array.from(new Set(payload.paths || []));

            if (unique.length === 0) {
              setContextInvalidPaths(payload.invalid || []);
              setIsContextMode(false);
              setResult({
                success: false,
                message:
                  "No valid local files or folders were provided from the context menu",
              });
              setTimeout(() => setResult(null), 3000);
              return;
            }

            setSelectedPaths(unique);
            setContextInvalidPaths(payload.invalid || []);
            setOperationMode("files");
            setIsContextMode(true);
            setIsWiping(false);
            setResult(null);
          },
        );
      } catch (error) {
        console.error("Error setting up context listener:", error);
      }
    };

    setupContextListener();

    return () => {
      if (unlistenContext) {
        unlistenContext();
      }
    };
  }, []);

  const handleFileSelect = async () => {
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        filters: [
          {
            name: "All Files",
            extensions: ["*"],
          },
        ],
        title: "Select Files to Securely Erase",
      });

      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];

        // Filter out empty paths and check for network paths
        const validPaths = paths.filter((path) => {
          if (!path.trim()) return false;
          if (path.startsWith("\\\\")) {
            showResult(false, "Network paths are not supported");
            return false;
          }
          return true;
        });

        // Check file sizes
        for (const path of validPaths) {
          try {
            const stats = await invoke("get_file_size", { path });
            if ((stats as number) > MAX_FILE_SIZE) {
              const confirmed = (await invoke("show_size_warning_dialog", {
                path,
                size: stats as number,
              })) as boolean;
              if (!confirmed) return;
            }
          } catch (error) {
            console.error("Error checking file size:", error);
          }
        }

        setSelectedPaths((prev) => [...new Set([...prev, ...validPaths])]);
        setOperationMode("files");
      }
    } catch (error) {
      console.error("Error selecting files:", error);
      showResult(false, `Error selecting files: ${error}`);
    }
  };

  const handleFolderSelect = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: true,
        title: "Select Folders to Securely Erase",
      });

      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];

        let hadNetworkPath = false;
        const validPaths = paths
          .map((path) => path.trim())
          .filter((path) => {
            if (!path) return false;
            if (path.startsWith("\\\\")) {
              hadNetworkPath = true;
              return false;
            }
            return true;
          });

        if (validPaths.length === 0) {
          showResult(
            false,
            hadNetworkPath
              ? "Network paths are not supported"
              : "No valid local folders were selected",
          );
          return;
        }

        setSelectedPaths((prev) => [
          ...new Set([...prev, ...validPaths]),
        ]);
        setOperationMode("files");
      }
    } catch (error) {
      console.error("Error selecting folders:", error);
      showResult(false, `Error selecting folders: ${error}`);
    }
  };

  const getAlgorithmDescription = () => {
    switch (algorithm) {
      case "NistClear":
        return "NIST 800-88 Clear method - Single pass with zeros (Quick)";
      case "NistPurge":
        return "NIST 800-88 Purge method - 3 passes with zeros, ones, and random data (Recommended)";
      case "Gutmann":
        return "Peter Gutmann's 35-pass algorithm - Maximum security for magnetic media (Very slow)";
      case "Random":
        return `${passes} passes of cryptographically secure random data (Custom)`;
    }
  };

  const showResult = (success: boolean, message: string) => {
    setResult({ success, message });
    setIsWiping(false);

    // Reset UI after 3 seconds
    setTimeout(() => {
      setResult(null);
      setSelectedPaths([]);
      setOperationMode("initial");
      setWipeProgress(null);
      setAbortController(null);
      setIsContextMode(false);
      setContextInvalidPaths([]);
    }, 3000);
  };

  const flashMessage = (success: boolean, message: string) => {
    setResult({ success, message });
    setTimeout(() => setResult(null), 2500);
  };

  const refreshContextMenuStatus = async () => {
    if (!isWindows) return;
    try {
      const status = (await invoke("get_context_menu_status")) as ContextMenuStatus;
      setContextMenuEnabled(status.enabled);
      setContextMenuMessage(status.message);
    } catch (error) {
      console.error("Unable to fetch context menu status:", error);
      setContextMenuMessage("Unable to query context menu status");
    }
  };

  const handleWipe = async () => {
    if (selectedPaths.length === 0) return;

    try {
      setIsContextMode(false);
      setResult(null);
      setWipeProgress(null);

      const confirmed = await invoke("show_confirmation_dialog", {
        path: selectedPaths.join("\n"),
        algorithm,
        description: getAlgorithmDescription(),
      });

      if (!confirmed) {
        showResult(false, "Operation cancelled by user");
        return;
      }

      setIsWiping(true);
      const controller = new AbortController();
      setAbortController(controller);

      const result = await invoke("wipe_files", {
        paths: selectedPaths,
        passes,
        algorithm,
      });

      setIsWiping(false);
      setAbortController(null);

      // Guard against undefined result
      if (!result) {
        showResult(false, "No response from wipe operation");
        return;
      }

      const { success = false, message = "Unknown error" } = result as {
        success?: boolean;
        message?: string;
      };
      showResult(success, message);
    } catch (error) {
      console.error("Error during wipe operation:", error);
      setIsWiping(false);
      setAbortController(null);
      showResult(false, `Error during wipe operation: ${error}`);
    }
  };

  const handleWipeFreeSpace = async () => {
    try {
      setResult(null);
      setWipeProgress(null);
      setOperationMode("freespace");

      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Drive to Wipe Free Space",
        defaultPath: "C:\\",
        buttonLabel: "Select Drive",
      });

      if (!selected) {
        showResult(false, "Operation cancelled by user");
        return;
      }

      const path = selected as string;
      const validationResult = await invoke("validate_drive_path", { path });
      const validation = validationResult as {
        success: boolean;
        message: string;
      };

      if (!validation.success) {
        showResult(false, validation.message);
        return;
      }

      const confirmed = await invoke("show_confirmation_dialog", {
        path,
        algorithm,
        description: getAlgorithmDescription(),
      });

      if (!confirmed) {
        showResult(false, "Operation cancelled by user");
        return;
      }

      setIsWiping(true);
      const controller = new AbortController();
      setAbortController(controller);

      const result = await invoke("execute_free_space_wipe", {
        path,
        algorithm,
        passes,
      });

      setIsWiping(false);
      setAbortController(null);

      // Guard against undefined result
      if (!result) {
        showResult(false, "No response from free space wipe operation");
        return;
      }

      const { success = false, message = "Unknown error" } = result as {
        success?: boolean;
        message?: string;
      };
      showResult(success, message);
    } catch (error) {
      console.error("Error during free space wipe:", error);
      setIsWiping(false);
      setAbortController(null);
      showResult(false, `Error during free space wipe: ${error}`);
    }
  };

  const handlePassesChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const value = e.target.value;
    if (value === "") {
      setPasses(1);
      return;
    }
    const numValue = parseInt(value);
    if (isNaN(numValue)) {
      return;
    }
    setPasses(Math.max(1, Math.min(35, numValue)));
  };

  const handleCancel = async () => {
    if (isWiping) {
      try {
        const window = new Window("main");
        await window.emit("cancel_operation");
        setIsWiping(false);
        setWipeProgress(null);
        showResult(false, "Operation cancelled by user");
      } catch (error) {
        console.error("Error cancelling operation:", error);
        showResult(false, `Error cancelling operation: ${error}`);
      }
    }
  };

  const toggleContextMenu = async (enable: boolean) => {
    if (!isWindows) return;
    setContextMenuBusy(true);
    try {
      const result = (await invoke(
        enable ? "register_context_menu" : "unregister_context_menu",
      )) as { success?: boolean; message?: string };
      await refreshContextMenuStatus();
      flashMessage(
        Boolean(result?.success),
        result?.message || "Context menu updated",
      );
    } catch (error) {
      console.error("Context menu update failed:", error);
      flashMessage(false, `Context menu update failed: ${error}`);
    } finally {
      setContextMenuBusy(false);
    }
  };

  return (
    <div className="min-h-screen bg-base-100 text-base-content">
      <div className="container mx-auto px-4 py-8 flex flex-col items-center max-h-screen overflow-hidden">
        {/* Header with Logo, Title, and Theme Toggle */}
        <div className="w-full flex justify-between items-center mb-8">
          <div className="flex-1" />
          <div className="text-center">
            <h1 className="text-4xl font-bold mb-2">BitBurn</h1>
            <p className="text-gray-400">Secure File & Drive Wiping Utility</p>
          </div>
          <div className="flex-1 flex justify-end">
            <button onClick={toggleTheme} className="btn btn-ghost btn-circle">
              {theme === "dark" ? (
                <svg
                  className="w-5 h-5"
                  fill="currentColor"
                  viewBox="0 0 20 20"
                >
                  <path d="M10 2a1 1 0 011 1v1a1 1 0 11-2 0V3a1 1 0 011-1zm4 8a4 4 0 11-8 0 4 4 0 018 0zm-.464 4.95l.707.707a1 1 0 001.414-1.414l-.707-.707a1 1 0 00-1.414 1.414zm2.12-10.607a1 1 0 010 1.414l-.706.707a1 1 0 11-1.414-1.414l.707-.707a1 1 0 011.414 0zM17 11a1 1 0 100-2h-1a1 1 0 100 2h1zm-7 4a1 1 0 011 1v1a1 1 0 11-2 0v-1a1 1 0 011-1zM5.05 6.464A1 1 0 106.465 5.05l-.708-.707a1 1 0 00-1.414 1.414l.707.707zm1.414 8.486l-.707.707a1 1 0 01-1.414-1.414l.707-.707a1 1 0 011.414 1.414zM4 11a1 1 0 100-2H3a1 1 0 000 2h1z" />
                </svg>
              ) : (
                <svg
                  className="w-5 h-5"
                  fill="currentColor"
                  viewBox="0 0 20 20"
                >
                  <path d="M17.293 13.293A8 8 0 016.707 2.707a8.001 8.001 0 1010.586 10.586z" />
                </svg>
              )}
            </button>
          </div>
        </div>

        {/* Back Button - Show when not in initial mode and not wiping */}
        {operationMode !== "initial" && !isWiping && (
          <button
            onClick={() => {
              setOperationMode("initial");
              setSelectedPaths([]);
            }}
            className="btn btn-ghost btn-sm mb-4 self-start"
          >
            ← Back to Operation Selection
          </button>
        )}

        {/* Description */}
        {operationMode === "initial" && (
          <div className="text-center mb-6">
            <h2 className="text-xl text-gray-200 mb-2">Choose an Operation</h2>
            <p className="text-gray-400">
              Select the type of secure wiping operation to perform
            </p>
          </div>
        )}

        {/* Create a flex container for the main content and notifications */}
        <div className="flex-1 w-full flex flex-col space-y-6 mb-12">
          {/* Settings Card - Always visible */}
          <div className="card bg-base-200 p-6 w-full">
            <div className="flex flex-col gap-6">
              {/* Algorithm Selection */}
              <div className="form-control">
                <label className="label justify-center">
                  <span className="label-text text-lg">Wipe Algorithm</span>
                </label>
                <select
                  value={algorithm}
                  onChange={(e) =>
                    setAlgorithm(e.target.value as typeof algorithm)
                  }
                  className="select select-bordered w-full text-lg text-center"
                  disabled={isWiping}
                >
                  <option value="NistClear">NIST 800-88 Clear (Quick)</option>
                  <option value="NistPurge">
                    NIST 800-88 Purge (Recommended)
                  </option>
                  <option value="Gutmann">Gutmann (Very Slow)</option>
                  <option value="Random">Random Passes (Custom)</option>
                </select>
                <div className="text-sm text-gray-400 text-center mt-2">
                  {getAlgorithmDescription()}
                </div>
              </div>

              <div className="form-control">
                <label className="label justify-center">
                  <span className="label-text text-lg">Number of Passes</span>
                </label>
                <div className="flex flex-col items-center gap-2">
                  {algorithm === "Random" ? (
                    <input
                      type="number"
                      min="1"
                      max="35"
                      value={passes}
                      onChange={handlePassesChange}
                      className="input input-bordered w-full text-lg text-center"
                      disabled={isWiping}
                    />
                  ) : (
                    <div className="text-3xl font-bold text-center text-primary">
                      {passes}
                    </div>
                  )}
                  <div className="text-sm text-gray-400 text-center">
                    {algorithm === "Random"
                      ? "Choose between 1-35 passes (3-7 recommended)"
                      : `Fixed at ${passes} ${passes === 1 ? "pass" : "passes"} for ${algorithm} algorithm`}
                  </div>
                </div>
              </div>
            </div>
          </div>

          {isWindows && (
            <div className="card bg-base-200 p-4 w-full">
              <div className="flex flex-col gap-2">
                <div className="flex items-center justify-between">
                  <div>
                    <h3 className="font-semibold text-lg">
                      Windows Explorer Context Menu
                    </h3>
                    <p className="text-sm text-gray-400">
                      Adds BitBurn -&gt; Shred -&gt; Choose Shred Algorithm to the
                      file/folder right-click menu.
                    </p>
                  </div>
                  <button
                    className="btn btn-outline btn-sm"
                    disabled={contextMenuBusy || contextMenuEnabled === null}
                    onClick={() => toggleContextMenu(!contextMenuEnabled)}
                  >
                    {contextMenuEnabled ? "Remove" : "Add"} Context Menu
                  </button>
                </div>
                {contextMenuMessage && (
                  <p className="text-sm text-gray-400">{contextMenuMessage}</p>
                )}
              </div>
            </div>
          )}

          {/* Operation Selection - Only visible in initial mode */}
          {operationMode === "initial" && !isWiping && (
            <div className="flex justify-center gap-4 mt-8">
              <button
                className="btn bg-[#3730a3] hover:bg-[#312e81] text-white border-none"
                onClick={() => setOperationMode("files")}
              >
                Wipe Files/Folders
              </button>
              <button
                className="btn bg-[#f97316] hover:bg-[#ea580c] text-white border-none"
                onClick={handleWipeFreeSpace}
              >
                Wipe Drive Free Space
              </button>
            </div>
          )}

          {/* Drop Zone - Only visible in files mode */}
          {operationMode === "files" && !isWiping && (
            <div
              className={`border-2 border-dashed rounded-lg p-8 w-full text-center transition-all duration-200 mb-6
                ${isDragging ? "border-primary bg-base-200 scale-102" : "hover:border-primary hover:bg-base-200/50"}`}
              onDrop={(e) => {
                e.preventDefault();
                e.stopPropagation();
              }}
              onDragOver={(e) => {
                e.preventDefault();
                setIsDragging(true);
              }}
              onDragLeave={(e) => {
                e.preventDefault();
                setIsDragging(false);
              }}
            >
              <div className="flex flex-col items-center justify-center">
                <svg
                  className="w-16 h-16 mb-4 text-gray-400"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth="2"
                    d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                  />
                </svg>
                <p className="text-xl text-gray-300 mb-4">
                  Drop files or folders here
                </p>
                <div className="flex gap-4">
                  <button
                    onClick={handleFileSelect}
                    className="btn btn-outline btn-sm"
                  >
                    Select Files
                  </button>
                  <button
                    onClick={handleFolderSelect}
                    className="btn btn-outline btn-sm"
                  >
                    Select Folders
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* Selected Files List - Only visible when files are selected */}
          {selectedPaths.length > 0 &&
            operationMode === "files" &&
            !isWiping && (
              <div className="card bg-base-200 p-4 w-full mb-16">
                <h3 className="font-semibold mb-2 text-lg text-center">
                  Selected Items ({selectedPaths.length})
                </h3>
                <div className="space-y-2 max-h-40 overflow-y-auto px-4">
                  {selectedPaths.map((path, index) => (
                    <div
                      key={index}
                      className="flex items-center justify-between gap-2 text-gray-300"
                    >
                      <p className="text-sm break-all flex-1 text-center">
                        {path}
                      </p>
                      <button
                        onClick={() =>
                          setSelectedPaths((paths) =>
                            paths.filter((_, i) => i !== index),
                          )
                        }
                        className="btn btn-ghost btn-xs text-gray-400 hover:text-error"
                      >
                        ×
                      </button>
                    </div>
                  ))}
                </div>
                <div className="mt-4 flex justify-center gap-4">
                  <button
                    className="btn btn-outline btn-sm"
                    onClick={() => {
                      setSelectedPaths([]);
                      setOperationMode("initial");
                    }}
                  >
                    Cancel
                  </button>
                  <button className="btn btn-error btn-sm" onClick={handleWipe}>
                    Securely Wipe Selected Items
                  </button>
                </div>
              </div>
            )}

          {isContextMode && !isWiping && (
            <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/60 px-4">
              <div className="card bg-base-200 w-full max-w-2xl shadow-2xl">
                <div className="card-body space-y-4">
                  <div className="flex items-center justify-between">
                    <h3 className="text-xl font-semibold">Context Menu Shred</h3>
                    <button
                      className="btn btn-ghost btn-sm"
                      onClick={() => {
                        setIsContextMode(false);
                        setContextInvalidPaths([]);
                      }}
                    >
                      ✕
                    </button>
                  </div>

                  <p className="text-gray-300">
                    Ready to shred {selectedPaths.length} {selectedPaths.length === 1 ? "item" : "items"}
                    {contextInvalidPaths.length > 0 ? " (some items were skipped)" : ""}.
                  </p>

                  {contextInvalidPaths.length > 0 && (
                    <div className="alert alert-warning text-sm">
                      <div className="font-semibold">Skipped items</div>
                      <ul className="list-disc list-inside space-y-1">
                        {contextInvalidPaths.map((issue, idx) => (
                          <li key={idx} className="break-all">{issue}</li>
                        ))}
                      </ul>
                    </div>
                  )}

                  <div className="max-h-40 overflow-y-auto bg-base-300/40 rounded p-3">
                    {selectedPaths.map((path, idx) => (
                      <div key={idx} className="text-sm text-gray-200 break-all">
                        {path}
                      </div>
                    ))}
                  </div>

                  <div className="flex justify-end gap-3 pt-2">
                    <button
                      className="btn btn-ghost"
                      onClick={() => {
                        setIsContextMode(false);
                        setContextInvalidPaths([]);
                        setSelectedPaths([]);
                        setOperationMode("initial");
                      }}
                    >
                      Cancel
                    </button>
                    <button className="btn btn-error" onClick={handleWipe}>
                      Shred Now
                    </button>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Progress Display */}
          {isWiping && wipeProgress && (
            <div className="card bg-base-200 p-6 w-full mb-6">
              <div className="text-center mb-4">
                <p className="text-xl font-semibold mb-2">
                  {wipeProgress.current_algorithm}
                </p>
                <p className="text-base text-gray-300 mb-1">
                  Pass {wipeProgress.current_pass} of{" "}
                  {wipeProgress.total_passes}
                </p>
                {operationMode === "files" && selectedPaths.length > 0 && (
                  <div className="text-sm text-gray-400 mb-2">
                    Wiping {selectedPaths.length}{" "}
                    {selectedPaths.length === 1 ? "item" : "items"}:
                    <div className="max-h-20 overflow-y-auto mt-1">
                      {selectedPaths.map((path, index) => (
                        <div
                          key={index}
                          className="text-xs text-gray-500 truncate"
                        >
                          {path}
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>

              <div className="w-full bg-base-300 rounded-lg h-4 mb-3">
                <div
                  className="bg-primary h-full rounded-lg transition-all duration-300 ease-linear"
                  style={{
                    width: `${Math.min(wipeProgress.percentage, 100)}%`,
                  }}
                />
              </div>

              <div className="flex justify-between items-center text-sm mb-2">
                <span className="text-gray-300">
                  {wipeProgress.current_pattern}
                </span>
                <span className="text-gray-300 font-medium">
                  {`${Math.min(Math.round(wipeProgress.percentage), 100)}%`}
                </span>
              </div>

              {(wipeProgress.total_bytes > 0 ||
                wipeProgress.estimated_total_bytes) && (
                <div className="text-center text-sm mb-4">
                  <span className="text-gray-300">
                    {(wipeProgress.bytes_processed / (1024 * 1024)).toFixed(2)}{" "}
                    MB
                  </span>
                  <span className="text-gray-500"> of </span>
                  <span className="text-gray-300">
                    {(
                      (wipeProgress.estimated_total_bytes ||
                        wipeProgress.total_bytes) /
                      (1024 * 1024)
                    ).toFixed(2)}{" "}
                    MB
                  </span>
                  <span className="text-gray-500"> processed</span>
                </div>
              )}

              {/* Cancel Button */}
              <div className="text-center mt-4">
                <button
                  className="btn btn-error btn-sm"
                  onClick={handleCancel}
                  disabled={!abortController}
                >
                  Cancel Operation
                </button>
              </div>
            </div>
          )}

          {/* Result Message - Positioned above warning footer */}
          {result && (
            <div className="fixed bottom-[60px] left-1/2 transform -translate-x-1/2 z-50 w-auto min-w-[300px] max-w-[90%]">
              <div
                className={`alert ${result.success ? "alert-success" : "alert-error"} shadow-lg flex justify-center`}
              >
                <div className="w-full text-center px-4">{result.message}</div>
              </div>
            </div>
          )}
        </div>

        {/* Warning Footer - Fixed at bottom with improved spacing */}
        <div className="fixed bottom-0 left-0 right-0">
          <div className="bg-base-300 h-px w-full opacity-30" />
          <div className="bg-base-100 p-4 text-center">
            <p className="text-gray-500 text-sm">
              ⚠️ Warning: Files erased with BitBurn cannot be recovered
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;
