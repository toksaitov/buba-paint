import { beforeEach, describe, expect, it, vi, type Mock } from "vitest";

vi.mock("../api", () => ({
  get: vi.fn(() => Promise.resolve("GET")),
  post: vi.fn(() => Promise.resolve("POST")),
  patch: vi.fn(() => Promise.resolve("PATCH")),
  del: vi.fn(() => Promise.resolve("DEL")),
  getText: vi.fn(() => Promise.resolve("TEXT")),
}));

import { del, get, getText, patch, post } from "../api";
import * as research from "../research-api";

const getMock = get as unknown as Mock;
const postMock = post as unknown as Mock;
const patchMock = patch as unknown as Mock;
const delMock = del as unknown as Mock;
const getTextMock = getText as unknown as Mock;

beforeEach(() => {
  getMock.mockClear();
  postMock.mockClear();
  patchMock.mockClear();
  delMock.mockClear();
  getTextMock.mockClear();
});

describe("research-api GET wrappers", () => {
  const cases: Array<[string, () => Promise<unknown>, string]> = [
    ["listResearchMachines", () => research.listResearchMachines(), "/api/research/machines"],
    ["getResearchMachine", () => research.getResearchMachine("m 1"), "/api/research/machines/m%201"],
    ["getResearchMachineHealth", () => research.getResearchMachineHealth("m1"), "/api/research/machines/m1/health"],
    ["getResearchMachineTelemetry", () => research.getResearchMachineTelemetry("m1"), "/api/research/machines/m1/telemetry"],
    ["listResearchArtifacts", () => research.listResearchArtifacts(), "/api/research/artifacts"],
    ["getResearchArtifact", () => research.getResearchArtifact("a1"), "/api/research/artifacts/a1"],
    ["getResearchArtifactManifest", () => research.getResearchArtifactManifest("a1"), "/api/research/artifacts/a1/manifest"],
    ["listArtifactTransfers", () => research.listArtifactTransfers(), "/api/research/transfers"],
    ["getArtifactTransfer", () => research.getArtifactTransfer("t1"), "/api/research/transfers/t1"],
    ["listResearchJobs", () => research.listResearchJobs(), "/api/research/jobs"],
    ["listResearchJobTemplates", () => research.listResearchJobTemplates(), "/api/research/job-templates"],
    ["getResearchQueue", () => research.getResearchQueue(), "/api/research/queue"],
    ["getResearchRetention", () => research.getResearchRetention(), "/api/research/retention"],
    ["getResearchJob", () => research.getResearchJob("j1"), "/api/research/jobs/j1"],
    ["listResearchReports", () => research.listResearchReports(), "/api/research/reports"],
    ["getResearchReport", () => research.getResearchReport("r1"), "/api/research/reports/r1"],
    ["getResearchReportJson", () => research.getResearchReportJson("r1"), "/api/research/reports/r1/json"],
  ];

  it.each(cases)("%s issues the expected GET", async (_name, call, path) => {
    await call();
    expect(getMock).toHaveBeenCalledWith(path);
  });
});

describe("research-api getText wrappers", () => {
  it("getResearchArtifactChecksums reads text", async () => {
    await research.getResearchArtifactChecksums("a1");
    expect(getTextMock).toHaveBeenCalledWith("/api/research/artifacts/a1/checksums");
  });

  it("getResearchReportCsv reads text", async () => {
    await research.getResearchReportCsv("r1");
    expect(getTextMock).toHaveBeenCalledWith("/api/research/reports/r1/csv");
  });
});

describe("research-api POST wrappers without a body", () => {
  const cases: Array<[string, () => Promise<unknown>, string]> = [
    ["verifyResearchArtifact", () => research.verifyResearchArtifact("a1"), "/api/research/artifacts/a1/verify"],
    ["archiveResearchArtifact", () => research.archiveResearchArtifact("a1"), "/api/research/artifacts/a1/archive"],
    ["restoreResearchArtifact", () => research.restoreResearchArtifact("a1"), "/api/research/artifacts/a1/restore"],
    ["cancelArtifactTransfer", () => research.cancelArtifactTransfer("t1"), "/api/research/transfers/t1/cancel"],
    ["pauseArtifactTransfer", () => research.pauseArtifactTransfer("t1"), "/api/research/transfers/t1/pause"],
    ["resumeArtifactTransfer", () => research.resumeArtifactTransfer("t1"), "/api/research/transfers/t1/resume"],
    ["verifyArtifactTransfer", () => research.verifyArtifactTransfer("t1"), "/api/research/transfers/t1/verify"],
    ["archiveResearchJobTemplate", () => research.archiveResearchJobTemplate("tpl1"), "/api/research/job-templates/tpl1/archive"],
    ["restoreResearchJobTemplate", () => research.restoreResearchJobTemplate("tpl1"), "/api/research/job-templates/tpl1/restore"],
    ["cancelResearchJob", () => research.cancelResearchJob("j1"), "/api/research/jobs/j1/cancel"],
    ["pauseResearchJob", () => research.pauseResearchJob("j1"), "/api/research/jobs/j1/pause"],
    ["resumeResearchJob", () => research.resumeResearchJob("j1"), "/api/research/jobs/j1/resume"],
    ["continueResearchJob", () => research.continueResearchJob("j1"), "/api/research/jobs/j1/continue"],
    ["retryResearchJob", () => research.retryResearchJob("j1"), "/api/research/jobs/j1/retry"],
    ["regenerateResearchJobReport", () => research.regenerateResearchJobReport("j1"), "/api/research/jobs/j1/report/regenerate"],
    ["archiveResearchJobScratch", () => research.archiveResearchJobScratch("j1"), "/api/research/jobs/j1/archive-scratch"],
    ["retryResearchStep", () => research.retryResearchStep("j1", "s1"), "/api/research/jobs/j1/steps/s1/retry"],
    ["cancelResearchStep", () => research.cancelResearchStep("j1", "s1"), "/api/research/jobs/j1/steps/s1/cancel"],
    ["clearResearchStepLease", () => research.clearResearchStepLease("j1", "s1"), "/api/research/jobs/j1/steps/s1/clear-lease"],
    ["resolveResearchStepBlocker", () => research.resolveResearchStepBlocker("j1", "s1"), "/api/research/jobs/j1/steps/s1/resolve-blocker"],
    ["archiveResearchReport", () => research.archiveResearchReport("r1"), "/api/research/reports/r1/archive"],
    ["restoreResearchReport", () => research.restoreResearchReport("r1"), "/api/research/reports/r1/restore"],
  ];

  it.each(cases)("%s issues the expected POST", async (_name, call, path) => {
    await call();
    expect(postMock).toHaveBeenCalledWith(path);
  });
});

describe("research-api POST wrappers with a body", () => {
  it("importResearchArtifact", async () => {
    const req = { artifact_root: "/root" };
    await research.importResearchArtifact(req);
    expect(postMock).toHaveBeenCalledWith("/api/research/artifacts/import", req);
  });

  it("registerResearchArtifact", async () => {
    const req = { manifest_json: "{}" };
    await research.registerResearchArtifact(req);
    expect(postMock).toHaveBeenCalledWith("/api/research/artifacts/register", req);
  });

  it("createArtifactTransfer", async () => {
    const req = {
      artifact_id: "a1",
      source_machine_id: "m1",
      destination_machine_id: "m2",
    };
    await research.createArtifactTransfer(req);
    expect(postMock).toHaveBeenCalledWith("/api/research/transfers", req);
  });

  it("createResearchJob", async () => {
    const req = { job_type: "export" as const };
    await research.createResearchJob(req);
    expect(postMock).toHaveBeenCalledWith("/api/research/jobs", req);
  });

  it("createResearchJobTemplate", async () => {
    const req = { name: "tpl", job_type: "sweep" as const, payload_json: "{}" };
    await research.createResearchJobTemplate(req);
    expect(postMock).toHaveBeenCalledWith("/api/research/job-templates", req);
  });

  it("archiveResearchRetention", async () => {
    const req = { artifact_ids: ["a1"], report_ids: [], delete_files: false };
    await research.archiveResearchRetention(req);
    expect(postMock).toHaveBeenCalledWith("/api/research/retention/archive", req);
  });

  it("appendResearchJobEvent", async () => {
    const req = { level: "info" as const, message: "note" };
    await research.appendResearchJobEvent("j1", req);
    expect(postMock).toHaveBeenCalledWith("/api/research/jobs/j1/events", req);
  });

  it("retryArtifactTransfer defaults the body to an empty object", async () => {
    await research.retryArtifactTransfer("t1");
    expect(postMock).toHaveBeenCalledWith("/api/research/transfers/t1/retry", {});
  });

  it("retryArtifactTransfer forwards a provided body", async () => {
    const req = { reset_progress: true };
    await research.retryArtifactTransfer("t1", req);
    expect(postMock).toHaveBeenCalledWith("/api/research/transfers/t1/retry", req);
  });

  it("cloneResearchJob defaults the body to an empty object", async () => {
    await research.cloneResearchJob("j1");
    expect(postMock).toHaveBeenCalledWith("/api/research/jobs/j1/clone", {});
  });

  it("cloneResearchJob forwards a provided body", async () => {
    const req = { priority: 5 };
    await research.cloneResearchJob("j1", req);
    expect(postMock).toHaveBeenCalledWith("/api/research/jobs/j1/clone", req);
  });
});

describe("research-api PATCH wrappers", () => {
  it("updateResearchArtifact", async () => {
    const req = { run_mode: "paper" };
    await research.updateResearchArtifact("a1", req);
    expect(patchMock).toHaveBeenCalledWith("/api/research/artifacts/a1", req);
  });

  it("updateResearchJobTemplate", async () => {
    const req = { name: "tpl", job_type: "sweep" as const, payload_json: "{}" };
    await research.updateResearchJobTemplate("tpl1", req);
    expect(patchMock).toHaveBeenCalledWith("/api/research/job-templates/tpl1", req);
  });

  it("updateResearchReport", async () => {
    const req = { title: "Renamed" };
    await research.updateResearchReport("r1", req);
    expect(patchMock).toHaveBeenCalledWith("/api/research/reports/r1", req);
  });
});

describe("research-api DELETE wrappers", () => {
  it("deleteArtifactTransfer", async () => {
    await research.deleteArtifactTransfer("t1");
    expect(delMock).toHaveBeenCalledWith("/api/research/transfers/t1");
  });

  it("deleteResearchJobTemplate", async () => {
    await research.deleteResearchJobTemplate("tpl1");
    expect(delMock).toHaveBeenCalledWith("/api/research/job-templates/tpl1");
  });

  it("deleteResearchJob", async () => {
    await research.deleteResearchJob("j1");
    expect(delMock).toHaveBeenCalledWith("/api/research/jobs/j1");
  });

  it("deleteResearchArtifact without files passes no params", async () => {
    await research.deleteResearchArtifact("a1", false);
    expect(delMock).toHaveBeenCalledWith("/api/research/artifacts/a1", undefined);
  });

  it("deleteResearchArtifact with files passes the delete_files flag", async () => {
    await research.deleteResearchArtifact("a1", true);
    expect(delMock).toHaveBeenCalledWith("/api/research/artifacts/a1", {
      delete_files: "true",
    });
  });

  it("deleteResearchReport without files passes no params", async () => {
    await research.deleteResearchReport("r1", false);
    expect(delMock).toHaveBeenCalledWith("/api/research/reports/r1", undefined);
  });

  it("deleteResearchReport with files passes the delete_files flag", async () => {
    await research.deleteResearchReport("r1", true);
    expect(delMock).toHaveBeenCalledWith("/api/research/reports/r1", {
      delete_files: "true",
    });
  });
});

describe("downloadResearchReportCsvFromText", () => {
  it("creates an object URL and triggers an anchor download", () => {
    const createObjectURL = vi.fn(() => "blob:fixture");
    const revokeObjectURL = vi.fn();
    vi.stubGlobal("URL", {
      ...URL,
      createObjectURL,
      revokeObjectURL,
    });
    const clickSpy = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => {});

    research.downloadResearchReportCsvFromText("a,b\n1,2\n", "report.csv");

    expect(createObjectURL).toHaveBeenCalledTimes(1);
    expect(clickSpy).toHaveBeenCalledTimes(1);
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:fixture");

    clickSpy.mockRestore();
    vi.unstubAllGlobals();
  });
});
