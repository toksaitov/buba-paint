import { beforeEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import {
  useRememberResearchListReturn,
  useResearchReturnTo,
} from "../use-research-return-to";

function RememberJobsList() {
  useRememberResearchListReturn("jobs", "/research/jobs");
  return <div>remembered</div>;
}

function JobsBackLink() {
  const returnTo = useResearchReturnTo("jobs", "/research/jobs");
  return <a href={returnTo}>Back to jobs</a>;
}

beforeEach(() => {
  window.sessionStorage.clear();
});

describe("research return links", () => {
  it("remembers list query state for detail back links", () => {
    render(
      <MemoryRouter initialEntries={["/research/jobs?preset=completed&type=sweep"]}>
        <RememberJobsList />
      </MemoryRouter>,
    );

    render(
      <MemoryRouter initialEntries={["/research/jobs/job-1"]}>
        <JobsBackLink />
      </MemoryRouter>,
    );

    expect(screen.getByRole("link", { name: /back to jobs/i })).toHaveAttribute(
      "href",
      "/research/jobs?preset=completed&type=sweep",
    );
  });

  it("rejects external stored return targets", () => {
    window.sessionStorage.setItem("buba.research.return.jobs", "https://bad");

    render(
      <MemoryRouter initialEntries={["/research/jobs/job-1"]}>
        <JobsBackLink />
      </MemoryRouter>,
    );

    expect(screen.getByRole("link", { name: /back to jobs/i })).toHaveAttribute(
      "href",
      "/research/jobs",
    );
  });

  it("rejects same-prefix non-list return targets", () => {
    window.sessionStorage.setItem(
      "buba.research.return.jobs",
      "/research/jobs-extra?q=bad",
    );

    render(
      <MemoryRouter initialEntries={["/research/jobs/job-1"]}>
        <JobsBackLink />
      </MemoryRouter>,
    );

    expect(screen.getByRole("link", { name: /back to jobs/i })).toHaveAttribute(
      "href",
      "/research/jobs",
    );
  });
});
