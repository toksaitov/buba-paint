import { describe, expect, it } from "vitest";
import {
  artifactTone,
  canPerform,
  checksumTone,
  getActionGateState,
  getAllowedActions,
  isJobTerminal,
  isStepLeaseExpired,
  isTransferTerminal,
  jobTone,
  machineTone,
  permissionHint,
  progressTone,
  reportTone,
  stepTone,
  transferTone,
} from "../research-permissions";

describe("canPerform", () => {
  it("admin can do every action", () => {
    expect(canPerform("admin", "delete_with_files")).toBe(true);
    expect(canPerform("admin", "regenerate_report")).toBe(true);
    expect(canPerform("admin", "clear_lease")).toBe(true);
  });

  it("observer can read and inspect health only", () => {
    expect(canPerform("observer", "read")).toBe(true);
    expect(canPerform("observer", "health")).toBe(true);
  });

  it("observer cannot mutate any state", () => {
    const mutations = [
      "create",
      "update",
      "delete",
      "delete_with_files",
      "cancel",
      "pause",
      "resume",
      "continue",
      "retry",
      "clone",
      "verify",
      "enable",
      "disable",
      "import",
      "register",
      "archive",
      "restore",
      "clear_lease",
      "resolve_blocker",
      "append_event",
      "regenerate_report",
    ] as const;
    for (const action of mutations) {
      expect(canPerform("observer", action)).toBe(false);
    }
  });
});

describe("getActionGateState", () => {
  it("enabled when admin has the action", () => {
    expect(getActionGateState("admin", "cancel")).toBe("enabled");
  });

  it("disabled_hint when observer attempts mutation", () => {
    expect(getActionGateState("observer", "cancel")).toBe("disabled_hint");
  });

  it("disabled_hint when role is undefined", () => {
    expect(getActionGateState(undefined, "read")).toBe("disabled_hint");
  });
});

describe("permissionHint", () => {
  it("returns admin hint for mutations", () => {
    expect(permissionHint("delete")).toMatch(/admin/i);
  });

  it("returns empty for observer-allowed actions", () => {
    expect(permissionHint("read")).toBe("");
    expect(permissionHint("health")).toBe("");
  });
});

describe("getAllowedActions - job", () => {
  it("queued job allows update/cancel/pause/append_event", () => {
    expect(getAllowedActions("job", "queued").sort()).toEqual(
      ["append_event", "cancel", "pause", "update"].sort(),
    );
  });

  it("running job allows cancel and append_event only", () => {
    expect(getAllowedActions("job", "running").sort()).toEqual(
      ["append_event", "cancel"].sort(),
    );
  });

  it("blocked job allows retry/continue/cancel/clone/append_event", () => {
    expect(getAllowedActions("job", "blocked").sort()).toEqual(
      ["append_event", "cancel", "clone", "continue", "retry"].sort(),
    );
  });

  it("failed job with no reports allows delete; with reports it does not", () => {
    expect(getAllowedActions("job", "failed").includes("delete")).toBe(true);
    expect(
      getAllowedActions("job", "failed", { has_reports: true }).includes(
        "delete",
      ),
    ).toBe(false);
  });

  it("completed job exposes clone, regenerate_report and delete (when no reports)", () => {
    const actions = getAllowedActions("job", "completed");
    expect(actions).toContain("clone");
    expect(actions).toContain("regenerate_report");
    expect(actions).toContain("delete");
  });
});

describe("getAllowedActions - step", () => {
  it("queued step only allows cancel", () => {
    expect(getAllowedActions("step", "queued")).toEqual(["cancel"]);
  });

  it("blocked step allows retry/cancel/resolve_blocker", () => {
    expect(getAllowedActions("step", "blocked").sort()).toEqual(
      ["cancel", "resolve_blocker", "retry"].sort(),
    );
  });

  it("leased step with active lease does NOT allow clear_lease", () => {
    const future = Date.now() + 60_000;
    expect(
      getAllowedActions("step", "leased", {
        leased_until_ms: future,
        now_ms: Date.now(),
      }),
    ).toEqual(["cancel"]);
  });

  it("leased step with expired lease offers clear_lease", () => {
    const past = Date.now() - 1_000;
    const actions = getAllowedActions("step", "leased", {
      leased_until_ms: past,
      now_ms: Date.now(),
    });
    expect(actions).toContain("clear_lease");
    expect(actions).toContain("cancel");
  });

  it("completed step exposes no actions", () => {
    expect(getAllowedActions("step", "completed")).toEqual([]);
  });
});

describe("getAllowedActions - transfer", () => {
  it("running offers cancel and pause", () => {
    expect(getAllowedActions("transfer", "running").sort()).toEqual(
      ["cancel", "pause"].sort(),
    );
  });

  it("retryable offers retry and cancel only", () => {
    expect(getAllowedActions("transfer", "retryable").sort()).toEqual(
      ["cancel", "retry"].sort(),
    );
  });

  it("completed offers verify and delete", () => {
    expect(getAllowedActions("transfer", "completed").sort()).toEqual(
      ["delete", "verify"].sort(),
    );
  });
});

describe("getAllowedActions - artifact", () => {
  it("available artifact offers update/verify/archive/delete/delete_with_files", () => {
    const actions = getAllowedActions("artifact", "available");
    expect(actions).toContain("update");
    expect(actions).toContain("verify");
    expect(actions).toContain("archive");
    expect(actions).toContain("delete");
    expect(actions).toContain("delete_with_files");
  });

  it("archived artifact offers restore + delete options only", () => {
    const actions = getAllowedActions("artifact", "archived");
    expect(actions).toContain("restore");
    expect(actions).toContain("delete");
    expect(actions).toContain("delete_with_files");
    expect(actions).not.toContain("archive");
    expect(actions).not.toContain("verify");
  });
});

describe("getAllowedActions - machine", () => {
  it("disabled machine offers enable + health", () => {
    const actions = getAllowedActions("machine", "disabled");
    expect(actions).toContain("enable");
    expect(actions).toContain("health");
    expect(actions).not.toContain("disable");
  });

  it("default machines cannot be deleted", () => {
    const actions = getAllowedActions("machine", "online", {
      is_default_machine: true,
    });
    expect(actions).not.toContain("delete");
  });

  it("dependent custom machines cannot be deleted", () => {
    const actions = getAllowedActions("machine", "online", {
      has_dependencies: true,
    });
    expect(actions).not.toContain("delete");
  });

  it("independent custom machines can be deleted", () => {
    const actions = getAllowedActions("machine", "online", {
      is_default_machine: false,
      has_dependencies: false,
    });
    expect(actions).toContain("delete");
  });
});

describe("getAllowedActions - report", () => {
  it("available report offers update/archive/delete/delete_with_files", () => {
    expect(getAllowedActions("report", "available").sort()).toEqual(
      ["archive", "delete", "delete_with_files", "update"].sort(),
    );
  });

  it("archived report offers restore + delete options", () => {
    const actions = getAllowedActions("report", "archived");
    expect(actions).toContain("restore");
    expect(actions).toContain("delete");
    expect(actions).toContain("delete_with_files");
  });
});

describe("terminal-state helpers", () => {
  it("identifies terminal job statuses", () => {
    expect(isJobTerminal("completed")).toBe(true);
    expect(isJobTerminal("failed")).toBe(true);
    expect(isJobTerminal("cancelled")).toBe(true);
    expect(isJobTerminal("running")).toBe(false);
    expect(isJobTerminal("paused")).toBe(false);
  });

  it("identifies terminal transfer statuses", () => {
    expect(isTransferTerminal("completed")).toBe(true);
    expect(isTransferTerminal("cancelled")).toBe(true);
    expect(isTransferTerminal("failed")).toBe(true);
    expect(isTransferTerminal("running")).toBe(false);
  });

  it("isStepLeaseExpired uses leased_until_ms", () => {
    expect(isStepLeaseExpired({ leased_until_ms: 100 }, 200)).toBe(true);
    expect(isStepLeaseExpired({ leased_until_ms: 300 }, 200)).toBe(false);
    expect(isStepLeaseExpired({ leased_until_ms: null }, 200)).toBe(false);
  });
});

describe("tone helpers", () => {
  it("machineTone maps known statuses", () => {
    expect(machineTone("online")).toBe("success");
    expect(machineTone("error")).toBe("danger");
    expect(machineTone("disabled")).toBe("muted");
  });

  it("artifactTone reflects archive status", () => {
    expect(artifactTone("available")).toBe("success");
    expect(artifactTone("archived")).toBe("muted");
  });

  it("transferTone maps to chip tones", () => {
    expect(transferTone("completed")).toBe("success");
    expect(transferTone("failed")).toBe("danger");
    expect(transferTone("paused")).toBe("muted");
  });

  it("checksumTone handles null", () => {
    expect(checksumTone(null)).toBe("muted");
    expect(checksumTone("verified")).toBe("success");
    expect(checksumTone("failed")).toBe("danger");
  });

  it("jobTone reflects state", () => {
    expect(jobTone("completed")).toBe("success");
    expect(jobTone("failed")).toBe("danger");
    expect(jobTone("blocked")).toBe("danger");
  });

  it("stepTone reflects state", () => {
    expect(stepTone("running")).toBe("warning");
    expect(stepTone("leased")).toBe("warning");
    expect(stepTone("completed")).toBe("success");
  });

  it("reportTone reflects archive", () => {
    expect(reportTone("archived")).toBe("muted");
    expect(reportTone("available")).toBe("success");
  });

  it("progressTone narrows ChipTone to ProgressBar tones", () => {
    expect(progressTone("warning")).toBe("warning");
    expect(progressTone("danger")).toBe("danger");
    expect(progressTone("neutral")).toBe("neutral");
    expect(progressTone("muted")).toBeUndefined();
    expect(progressTone("success")).toBeUndefined();
  });
});
