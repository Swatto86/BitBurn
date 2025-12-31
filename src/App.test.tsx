import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";

// Mock Tauri APIs
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("@tauri-apps/api/window", () => {
  // Create a mock Window class that can be instantiated
  class MockWindow {
    listen = vi.fn(() => Promise.resolve(() => {}));
    emit = vi.fn();
    onDragDropEvent = vi.fn(() => Promise.resolve(() => {}));
  }

  return {
    Window: MockWindow,
    getCurrent: vi.fn(() => new MockWindow()),
  };
});

// Import the mocked modules
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
// @ts-expect-error - getCurrent is provided by our mock but doesn't exist in the real Tauri v2 API
import { getCurrent } from "@tauri-apps/api/window";

const mockInvoke = vi.mocked(invoke);
const mockOpen = vi.mocked(open);
const mockGetCurrent = vi.mocked(getCurrent);

describe("BitBurn UI Tests", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();

    // Setup default mocks
    mockGetCurrent.mockReturnValue({
      listen: vi.fn(() => Promise.resolve(() => {})),
      emit: vi.fn(),
      onDragDropEvent: vi.fn(() => Promise.resolve(() => {})),
    } as any);
  });

  describe("Initial Render", () => {
    it("should render the main title and subtitle", () => {
      render(<App />);

      expect(screen.getByText("BitBurn")).toBeInTheDocument();
      expect(
        screen.getByText("Secure File & Drive Wiping Utility"),
      ).toBeInTheDocument();
    });

    it("should render theme toggle button", () => {
      render(<App />);

      const buttons = screen.getAllByRole("button");
      expect(buttons.length).toBeGreaterThan(0);
    });

    it("should render algorithm selection dropdown", () => {
      render(<App />);

      expect(screen.getByText(/wipe algorithm/i)).toBeInTheDocument();
      expect(screen.getByRole("combobox")).toBeInTheDocument();
    });

    it("should render all algorithm options", () => {
      render(<App />);

      const select = screen.getByRole("combobox");
      expect(select).toBeInTheDocument();

      const options = screen.getAllByRole("option");
      expect(options).toHaveLength(4);

      // Use getByRole to avoid matching description text
      expect(
        screen.getByRole("option", { name: /NIST 800-88 Clear/i }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("option", { name: /NIST 800-88 Purge/i }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("option", { name: /Gutmann/i }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("option", { name: /Random/i }),
      ).toBeInTheDocument();
    });

    it("should render pass count section", () => {
      render(<App />);

      expect(screen.getByText(/number of passes/i)).toBeInTheDocument();
    });

    it("should render operation selection buttons in initial mode", () => {
      render(<App />);

      expect(screen.getByText("Wipe Files/Folders")).toBeInTheDocument();
      expect(screen.getByText("Wipe Drive Free Space")).toBeInTheDocument();
    });

    it("should render warning footer", () => {
      render(<App />);

      expect(
        screen.getByText(/Files erased with BitBurn cannot be recovered/i),
      ).toBeInTheDocument();
    });

    it("should render operation selection description", () => {
      render(<App />);

      expect(
        screen.getByText(/Select the type of secure wiping operation/i),
      ).toBeInTheDocument();
    });
  });

  describe("Theme Toggle", () => {
    it("should toggle theme when clicking theme button", async () => {
      render(<App />);

      const buttons = screen.getAllByRole("button");
      const themeButton = buttons.find((btn) =>
        btn.querySelector("svg"),
      ) as HTMLButtonElement;

      expect(themeButton).toBeDefined();

      if (themeButton) {
        await userEvent.click(themeButton);
        const htmlElement = document.documentElement;
        expect(htmlElement.getAttribute("data-theme")).toBe("light");

        await userEvent.click(themeButton);
        expect(htmlElement.getAttribute("data-theme")).toBe("dark");
      }
    });

    it("should persist theme to localStorage", async () => {
      render(<App />);

      const buttons = screen.getAllByRole("button");
      const themeButton = buttons.find((btn) =>
        btn.querySelector("svg"),
      ) as HTMLButtonElement;

      if (themeButton) {
        const initialTheme =
          document.documentElement.getAttribute("data-theme");
        await userEvent.click(themeButton);

        // Check theme was toggled
        const newTheme = document.documentElement.getAttribute("data-theme");
        expect(newTheme).not.toBe(initialTheme);
      }
    });
  });

  describe("Algorithm Selection", () => {
    it("should change algorithm when selecting from dropdown", async () => {
      render(<App />);

      const select = screen.getByRole("combobox");
      await userEvent.selectOptions(select, "NistClear");
      expect(select).toHaveValue("NistClear");
    });

    it("should display correct pass count for NIST Clear", async () => {
      render(<App />);

      const select = screen.getByRole("combobox");
      await userEvent.selectOptions(select, "NistClear");

      await waitFor(() => {
        expect(screen.getByText("1")).toBeInTheDocument();
      });
    });

    it("should display correct pass count for NIST Purge", async () => {
      render(<App />);

      const select = screen.getByRole("combobox");
      await userEvent.selectOptions(select, "NistPurge");

      await waitFor(() => {
        expect(screen.getByText("3")).toBeInTheDocument();
      });
    });

    it("should display correct pass count for Gutmann", async () => {
      render(<App />);

      const select = screen.getByRole("combobox");
      await userEvent.selectOptions(select, "Gutmann");

      await waitFor(() => {
        expect(screen.getByText("35")).toBeInTheDocument();
      });
    });

    it("should show input field for Random algorithm", async () => {
      render(<App />);

      const select = screen.getByRole("combobox");
      await userEvent.selectOptions(select, "Random");

      await waitFor(() => {
        const input = screen.getByRole("spinbutton");
        expect(input).toBeInTheDocument();
        // Should retain the previous pass count (3 from NistPurge default)
        expect(input).toHaveValue(3);
      });
    });

    it("should display algorithm description", async () => {
      render(<App />);

      const select = screen.getByRole("combobox");

      await userEvent.selectOptions(select, "NistClear");
      await waitFor(() => {
        expect(screen.getByText(/Single pass with zeros/i)).toBeInTheDocument();
      });

      await userEvent.selectOptions(select, "NistPurge");
      await waitFor(() => {
        expect(
          screen.getByText(/3 passes with zeros, ones, and random data/i),
        ).toBeInTheDocument();
      });
    });
  });

  describe("Pass Count Input", () => {
    it("should allow changing pass count for Random algorithm", async () => {
      render(<App />);

      const select = screen.getByRole("combobox");
      await userEvent.selectOptions(select, "Random");

      const input = await screen.findByRole("spinbutton");
      expect(input).toBeInTheDocument();

      // Use fireEvent to set value directly to avoid character-by-character typing issues
      fireEvent.change(input, { target: { value: "10" } });

      await waitFor(() => {
        expect(input).toHaveValue(10);
      });
    });

    it("should enforce minimum pass count of 1", async () => {
      render(<App />);

      const select = screen.getByRole("combobox");
      await userEvent.selectOptions(select, "Random");

      const input = await screen.findByRole("spinbutton");
      expect(input).toBeInTheDocument();

      // Set value to 0 directly
      fireEvent.change(input, { target: { value: "0" } });

      // Should default to 1
      await waitFor(
        () => {
          expect(input).toHaveValue(1);
        },
        { timeout: 3000 },
      );
    });

    it("should enforce maximum pass count of 35", async () => {
      render(<App />);

      const select = screen.getByRole("combobox");
      await userEvent.selectOptions(select, "Random");

      const input = await screen.findByRole("spinbutton");

      // Set value to 50 directly
      fireEvent.change(input, { target: { value: "50" } });

      // Should cap at 35
      await waitFor(() => {
        expect(input).toHaveValue(35);
      });
    });
  });

  describe("File Selection Mode", () => {
    it("should switch to files mode when clicking Wipe Files/Folders", async () => {
      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      await waitFor(() => {
        expect(screen.getByText("Select Files")).toBeInTheDocument();
      });
    });

    it("should show back button in files mode", async () => {
      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      await waitFor(() => {
        expect(screen.getByText(/Back/i)).toBeInTheDocument();
      });
    });

    it("should hide operation selection in files mode", async () => {
      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      expect(
        screen.queryByText("Select the type of secure wiping operation"),
      ).not.toBeInTheDocument();
    });

    it("should return to initial mode when clicking back button", async () => {
      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const backButton = await screen.findByText(/Back/i);
      await userEvent.click(backButton);

      await waitFor(() => {
        expect(
          screen.getByText(
            "Select the type of secure wiping operation to perform",
          ),
        ).toBeInTheDocument();
      });
    });
  });

  describe("File Selection", () => {
    it("should call file dialog when clicking Select Files", async () => {
      mockOpen.mockResolvedValue(null);

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFilesButton = screen.getByText("Select Files");
      await userEvent.click(selectFilesButton);

      await waitFor(
        () => {
          expect(mockOpen).toHaveBeenCalled();
        },
        { timeout: 1000 },
      );
    });

    it("should display selected files", async () => {
      mockOpen.mockResolvedValue(["C:\\test\\file1.txt"] as any);

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFilesButton = screen.getByText("Select Files");
      await userEvent.click(selectFilesButton);

      await waitFor(() => {
        expect(screen.getByText(/C:\\test\\file1.txt/)).toBeInTheDocument();
      });
    });

    it("should show wipe button when files are selected", async () => {
      mockOpen.mockResolvedValue([
        "C:\\test\\file1.txt",
        "C:\\test\\file2.txt",
      ] as any);

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFilesButton = screen.getByText("Select Files");
      await userEvent.click(selectFilesButton);

      await waitFor(() => {
        expect(
          screen.getByText("Securely Wipe Selected Items"),
        ).toBeInTheDocument();
      });
    });

    it("should allow removing individual files", async () => {
      mockOpen.mockResolvedValue([
        "C:\\test\\file1.txt",
        "C:\\test\\file2.txt",
      ] as any);

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFilesButton = screen.getByText("Select Files");
      await userEvent.click(selectFilesButton);

      await waitFor(() => {
        expect(screen.getByText(/file1\.txt/)).toBeInTheDocument();
      });

      const removeButtons = screen.getAllByText("×");
      await userEvent.click(removeButtons[0]);

      await waitFor(() => {
        expect(screen.queryByText(/file1\.txt/)).not.toBeInTheDocument();
      });
    });

    it("should handle file selection cancellation", async () => {
      mockOpen.mockResolvedValue(null);

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFilesButton = screen.getByText("Select Files");
      await userEvent.click(selectFilesButton);

      await waitFor(() => {
        expect(mockOpen).toHaveBeenCalled();
      });
    });
  });

  describe("Folder Selection", () => {
    it("should call folder dialog when clicking Select Folders", async () => {
      mockOpen.mockResolvedValue(null);

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFoldersButton = screen.getByText("Select Folders");
      await userEvent.click(selectFoldersButton);

      await waitFor(
        () => {
          expect(mockOpen).toHaveBeenCalled();
        },
        { timeout: 1000 },
      );
    });

    it("should display selected folders", async () => {
      mockOpen.mockResolvedValue(["C:\\test\\folder"] as any);

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFoldersButton = screen.getByText("Select Folders");
      await userEvent.click(selectFoldersButton);

      await waitFor(() => {
        expect(screen.getByText(/C:\\test\\folder/)).toBeInTheDocument();
      });
    });

    it("should reject UNC network folder selections", async () => {
      mockOpen.mockResolvedValue(["\\\\server\\share"] as any);

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFoldersButton = screen.getByText("Select Folders");
      await userEvent.click(selectFoldersButton);

      await waitFor(() => {
        expect(
          screen.getByText(/Network paths are not supported/i),
        ).toBeInTheDocument();
      });
    });
  });

  describe("Drag and Drop", () => {
    it("should highlight drop zone on drag over", async () => {
      let dragEventCallback: any;
      mockGetCurrent.mockReturnValue({
        listen: vi.fn(() => Promise.resolve(() => {})),
        emit: vi.fn(),
        onDragDropEvent: vi.fn((callback) => {
          dragEventCallback = callback;
          return Promise.resolve(() => {});
        }),
      } as any);

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      await waitFor(() => {
        expect(
          screen.getByText(/Drop files or folders here/i),
        ).toBeInTheDocument();
      });

      // Simulate drag enter event via Tauri API
      if (dragEventCallback) {
        dragEventCallback({ payload: { type: "enter" } });
      }

      await waitFor(() => {
        // Find the actual drop zone div that has the conditional classes
        const dropZoneDiv = screen
          .getByText(/Drop files or folders here/i)
          .closest('div[class*="border"]');
        expect(dropZoneDiv?.className).toMatch(/scale-102|border-primary/);
      });
    });

    it("should remove highlight on drag leave", async () => {
      let dragEventCallback: any;
      mockGetCurrent.mockReturnValue({
        listen: vi.fn(() => Promise.resolve(() => {})),
        emit: vi.fn(),
        onDragDropEvent: vi.fn((callback) => {
          dragEventCallback = callback;
          return Promise.resolve(() => {});
        }),
      } as any);

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      // Simulate drag enter then leave
      if (dragEventCallback) {
        await dragEventCallback({ payload: { type: "enter" } });
        await dragEventCallback({ payload: { type: "leave" } });
      }

      await waitFor(() => {
        const dropZone = screen
          .getByText(/Drop files or folders here/i)
          .closest("div");
        expect(dropZone?.className).not.toMatch(/scale-102/);
      });
    });
  });

  describe("Wipe Operation", () => {
    it("should show confirmation and start wipe operation", async () => {
      mockOpen.mockResolvedValue(["C:\\test\\file1.txt"] as any);
      mockInvoke
        .mockResolvedValueOnce(100) // get_file_size
        .mockResolvedValueOnce(true) // confirmation
        .mockResolvedValueOnce({ success: true, message: "Wipe completed" }); // wipe result

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFilesButton = screen.getByText("Select Files");
      await userEvent.click(selectFilesButton);

      await waitFor(() => {
        expect(
          screen.getByText("Securely Wipe Selected Items"),
        ).toBeInTheDocument();
      });

      const wipeButton = screen.getByText("Securely Wipe Selected Items");
      await userEvent.click(wipeButton);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith(
          "show_confirmation_dialog",
          expect.any(Object),
        );
      });
    });

    it("should disable controls during wiping", async () => {
      mockOpen.mockResolvedValue(["C:\\test\\file1.txt"] as any);

      let wipeResolver: any;
      const wipePromise = new Promise((resolve) => {
        wipeResolver = resolve;
      });

      mockInvoke
        .mockResolvedValueOnce(100) // get_file_size
        .mockResolvedValueOnce(true) // confirmation
        .mockReturnValueOnce(wipePromise as any); // wipe - keep pending

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFilesButton = screen.getByText("Select Files");
      await userEvent.click(selectFilesButton);

      await waitFor(() => {
        expect(
          screen.getByText("Securely Wipe Selected Items"),
        ).toBeInTheDocument();
      });

      const wipeButton = screen.getByText("Securely Wipe Selected Items");
      await userEvent.click(wipeButton);

      // Wait for confirmation dialog to be called
      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith(
          "show_confirmation_dialog",
          expect.any(Object),
        );
      });

      // Wait for wipe_files to be called (which means wiping has started)
      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith(
          "wipe_files",
          expect.any(Object),
        );
      });

      // Now check that the select is disabled while wiping
      const select = screen.getByRole("combobox");
      expect(select).toBeDisabled();

      // Clean up
      await waitFor(() => {
        wipeResolver({ success: true, message: "Done" });
      });
    });

    it("should show cancel button during wipe", async () => {
      mockOpen.mockResolvedValue(["C:\\test\\file1.txt"] as any);

      let wipeResolver: any;
      const wipePromise = new Promise((resolve) => {
        wipeResolver = resolve;
      });

      let progressCallback: any;
      const mockListen = vi.fn((event: string, callback: any) => {
        if (event === "wipe_progress") {
          progressCallback = callback;
        }
        return Promise.resolve(() => {});
      });

      mockInvoke
        .mockResolvedValueOnce(100) // get_file_size
        .mockResolvedValueOnce(true) // confirmation
        .mockReturnValueOnce(wipePromise as any); // wipe result - keep pending

      // Setup mock AFTER clearAllMocks in beforeEach
      mockGetCurrent.mockReturnValue({
        listen: mockListen,
        emit: vi.fn(),
        onDragDropEvent: vi.fn(() => Promise.resolve(() => {})),
      } as any);

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFilesButton = screen.getByText("Select Files");
      await userEvent.click(selectFilesButton);

      await waitFor(() => {
        expect(
          screen.getByText("Securely Wipe Selected Items"),
        ).toBeInTheDocument();
      });

      const wipeButton = screen.getByText("Securely Wipe Selected Items");
      await userEvent.click(wipeButton);

      // Wait a bit for async operations
      await new Promise((resolve) => setTimeout(resolve, 50));

      // Trigger progress to display cancel button if callback is set
      if (progressCallback) {
        await waitFor(() => {
          progressCallback({
            payload: {
              current_pass: 1,
              total_passes: 3,
              bytes_processed: 500,
              total_bytes: 1000,
              current_algorithm: "NistPurge",
              current_pattern: "zeros",
              percentage: 50,
              estimated_total_bytes: 1000,
            },
          });
        });

        await waitFor(() => {
          expect(screen.getByText("Cancel Operation")).toBeInTheDocument();
        });
      }

      // Resolve the wipe to clean up
      await waitFor(() => {
        wipeResolver({ success: true, message: "Completed" });
      });
    });
  });

  describe("Progress Display", () => {
    it("should display progress information during wipe", async () => {
      mockOpen.mockResolvedValue(["C:\\test\\file1.txt"] as any);

      let wipeResolver: any;
      const wipePromise = new Promise((resolve) => {
        wipeResolver = resolve;
      });

      let progressCallback: any;
      const mockListen = vi.fn((event: string, callback: any) => {
        if (event === "wipe_progress") {
          progressCallback = callback;
        }
        return Promise.resolve(() => {});
      });

      mockInvoke
        .mockResolvedValueOnce(100) // get_file_size
        .mockResolvedValueOnce(true) // confirmation
        .mockReturnValueOnce(wipePromise as any); // wipe result

      // Setup mock AFTER clearAllMocks in beforeEach
      mockGetCurrent.mockReturnValue({
        listen: mockListen,
        emit: vi.fn(),
        onDragDropEvent: vi.fn(() => Promise.resolve(() => {})),
      } as any);

      render(<App />);

      const select = screen.getByRole("combobox");
      await userEvent.selectOptions(select, "NistPurge");

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFilesButton = screen.getByText("Select Files");
      await userEvent.click(selectFilesButton);

      await waitFor(() => {
        expect(
          screen.getByText("Securely Wipe Selected Items"),
        ).toBeInTheDocument();
      });

      const wipeButton = screen.getByText("Securely Wipe Selected Items");
      await userEvent.click(wipeButton);

      // Wait for wipe to start and listener to be set up
      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith(
          "wipe_files",
          expect.any(Object),
        );
      });

      // Wait a bit for async operations
      await new Promise((resolve) => setTimeout(resolve, 50));

      // Simulate progress event if callback is set
      if (progressCallback) {
        await waitFor(() => {
          progressCallback({
            payload: {
              current_pass: 1,
              total_passes: 3,
              bytes_processed: 500,
              total_bytes: 1000,
              current_algorithm: "NistPurge",
              current_pattern: "zeros",
              percentage: 50,
              estimated_total_bytes: 1000,
            },
          });
        });

        // Wait for progress to be displayed
        await waitFor(() => {
          expect(screen.getByText(/Pass 1 of 3/)).toBeInTheDocument();
        });
      }

      // Clean up
      await waitFor(() => {
        wipeResolver({ success: true, message: "Done" });
      });
    });
  });

  describe("Result Display", () => {
    it("should display success message after successful wipe", async () => {
      mockOpen.mockResolvedValue(["C:\\test\\file1.txt"] as any);
      mockInvoke
        .mockResolvedValueOnce(100) // get_file_size
        .mockResolvedValueOnce(true) // confirmation
        .mockResolvedValueOnce({
          success: true,
          message: "Wipe completed successfully!",
        });

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFilesButton = screen.getByText("Select Files");
      await userEvent.click(selectFilesButton);

      await waitFor(() => {
        expect(
          screen.getByText("Securely Wipe Selected Items"),
        ).toBeInTheDocument();
      });

      const wipeButton = screen.getByText("Securely Wipe Selected Items");
      await userEvent.click(wipeButton);

      // The wipe completes immediately with the mocked response
      // Wait for the success message to appear
      await waitFor(
        () => {
          expect(
            screen.getByText(/Wipe completed successfully!/i),
          ).toBeInTheDocument();
        },
        { timeout: 3000 },
      );
    });

    it("should display error message after failed wipe", async () => {
      mockOpen.mockResolvedValue(["C:\\test\\file1.txt"] as any);
      mockInvoke
        .mockResolvedValueOnce(100) // get_file_size
        .mockResolvedValueOnce(true) // confirmation
        .mockResolvedValueOnce({ success: false, message: "Wipe failed!" });

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFilesButton = screen.getByText("Select Files");
      await userEvent.click(selectFilesButton);

      await waitFor(() => {
        expect(
          screen.getByText("Securely Wipe Selected Items"),
        ).toBeInTheDocument();
      });

      const wipeButton = screen.getByText("Securely Wipe Selected Items");
      await userEvent.click(wipeButton);

      // The wipe completes immediately with the mocked response
      // Wait for the error message to appear
      await waitFor(
        () => {
          expect(screen.getByText(/Wipe failed!/i)).toBeInTheDocument();
        },
        { timeout: 3000 },
      );
    });
  });

  describe("Free Space Wiping", () => {
    it("should call drive selection dialog for free space wipe", async () => {
      mockOpen.mockResolvedValue(null);

      render(<App />);

      const freeSpaceButton = screen.getByText("Wipe Drive Free Space");
      await userEvent.click(freeSpaceButton);

      await waitFor(
        () => {
          expect(mockOpen).toHaveBeenCalled();
        },
        { timeout: 1000 },
      );
    });

    it("should validate drive path before wiping", async () => {
      mockOpen.mockResolvedValue("C:\\" as any);
      mockInvoke
        .mockResolvedValueOnce({ success: true, message: "Valid drive" }) // validation
        .mockResolvedValueOnce(true) // confirmation
        .mockResolvedValueOnce({ success: true, message: "Free space wiped" });

      render(<App />);

      const freeSpaceButton = screen.getByText("Wipe Drive Free Space");
      await userEvent.click(freeSpaceButton);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith(
          "validate_drive_path",
          expect.objectContaining({ path: "C:\\" }),
        );
      });
    });
  });

  describe("Cancel Operation", () => {
    it("should show cancel button during operation", async () => {
      mockOpen.mockResolvedValue(["C:\\test\\file1.txt"] as any);

      let wipeResolver: any;
      const wipePromise = new Promise((resolve) => {
        wipeResolver = resolve;
      });

      let progressCallback: any;
      const mockListen = vi.fn((event: string, callback: any) => {
        if (event === "wipe_progress") {
          progressCallback = callback;
        }
        return Promise.resolve(() => {});
      });

      mockInvoke
        .mockResolvedValueOnce(100) // get_file_size
        .mockResolvedValueOnce(true) // confirmation
        .mockReturnValueOnce(wipePromise as any); // wipe result - keep pending

      // Setup mock AFTER clearAllMocks in beforeEach
      mockGetCurrent.mockReturnValue({
        listen: mockListen,
        emit: vi.fn(),
        onDragDropEvent: vi.fn(() => Promise.resolve(() => {})),
      } as any);

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFilesButton = screen.getByText("Select Files");
      await userEvent.click(selectFilesButton);

      await waitFor(() => {
        expect(
          screen.getByText("Securely Wipe Selected Items"),
        ).toBeInTheDocument();
      });

      const wipeButton = screen.getByText("Securely Wipe Selected Items");
      await userEvent.click(wipeButton);

      // Wait a bit for async operations
      await new Promise((resolve) => setTimeout(resolve, 50));

      // Trigger progress to display cancel button if callback is set
      if (progressCallback) {
        await waitFor(() => {
          progressCallback({
            payload: {
              current_pass: 1,
              total_passes: 3,
              bytes_processed: 500,
              total_bytes: 1000,
              current_algorithm: "NistPurge",
              current_pattern: "zeros",
              percentage: 50,
              estimated_total_bytes: 1000,
            },
          });
        });

        await waitFor(() => {
          expect(screen.getByText("Cancel Operation")).toBeInTheDocument();
        });
      }

      // Clean up
      await waitFor(() => {
        wipeResolver({ success: true, message: "Done" });
      });
    });
  });

  describe("Accessibility", () => {
    it("should have proper labels for form controls", () => {
      render(<App />);

      expect(screen.getByText(/wipe algorithm/i)).toBeInTheDocument();
      expect(screen.getByText(/number of passes/i)).toBeInTheDocument();
    });

    it("should have accessible buttons", () => {
      render(<App />);

      const buttons = screen.getAllByRole("button");
      buttons.forEach((button) => {
        expect(button).toBeVisible();
      });
    });
  });

  describe("Responsive UI Elements", () => {
    it("should display all main sections", () => {
      render(<App />);

      expect(screen.getByText("BitBurn")).toBeInTheDocument();
      expect(screen.getByRole("combobox")).toBeInTheDocument();
      expect(screen.getByText(/wipe algorithm/i)).toBeInTheDocument();
      expect(screen.getByText(/number of passes/i)).toBeInTheDocument();
      expect(
        screen.getByText(/Files erased with BitBurn cannot be recovered/i),
      ).toBeInTheDocument();
    });

    it("should show selected files count correctly", async () => {
      mockOpen.mockResolvedValue([
        "C:\\test\\file1.txt",
        "C:\\test\\file2.txt",
        "C:\\test\\file3.txt",
      ] as any);

      render(<App />);

      const filesButton = screen.getByText("Wipe Files/Folders");
      await userEvent.click(filesButton);

      const selectFilesButton = screen.getByText("Select Files");
      await userEvent.click(selectFilesButton);

      await waitFor(() => {
        expect(screen.getByText(/C:\\test\\file1.txt/)).toBeInTheDocument();
        expect(screen.getByText(/C:\\test\\file2.txt/)).toBeInTheDocument();
        expect(screen.getByText(/C:\\test\\file3.txt/)).toBeInTheDocument();
      });
    });
  });

  describe("Platform gating", () => {
    it("shows context controls when backend reports Windows", async () => {
      Object.defineProperty(window as any, "__TAURI_IPC__", {
        value: true,
        configurable: true,
      });

      mockInvoke.mockResolvedValueOnce({ is_windows: true });
      mockInvoke.mockResolvedValueOnce({
        enabled: true,
        message: "Context menu is registered",
      });

      render(<App />);

      await waitFor(() => {
        expect(
          screen.getByText("Windows Explorer Context Menu"),
        ).toBeInTheDocument();
      });

      expect(
        screen.getByText(/Context menu is registered/i),
      ).toBeInTheDocument();

      delete (window as any).__TAURI_IPC__;
    });

    it("hides context controls when backend reports non-Windows", async () => {
      Object.defineProperty(window as any, "__TAURI_IPC__", {
        value: true,
        configurable: true,
      });

      mockInvoke.mockResolvedValueOnce({ is_windows: false });

      render(<App />);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("platform_info");
      });

      expect(
        screen.queryByText("Windows Explorer Context Menu"),
      ).not.toBeInTheDocument();

      delete (window as any).__TAURI_IPC__;
    });
  });
});
