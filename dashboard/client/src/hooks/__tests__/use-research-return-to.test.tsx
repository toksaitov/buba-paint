import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router-dom";
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

function CurrentLocation() {
  const location = useLocation();
  return (
    <div data-testid="location">
      {location.pathname}
      {location.search}
    </div>
  );
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

  it("restores stored list query state when entering a bare list route", async () => {
    window.sessionStorage.setItem(
      "buba.research.return.jobs",
      "/research/jobs?preset=completed&type=sweep",
    );

    render(
      <MemoryRouter initialEntries={["/research/jobs"]}>
        <RememberJobsList />
        <CurrentLocation />
      </MemoryRouter>,
    );

    await waitFor(() =>
      expect(screen.getByTestId("location")).toHaveTextContent(
        "/research/jobs?preset=completed&type=sweep",
      ),
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

  it("allows URL-like text inside same-page query parameters", () => {
    window.sessionStorage.setItem(
      "buba.research.return.jobs",
      "/research/jobs?q=https%3A%2F%2Fexample.test%2Fartifact",
    );

    render(
      <MemoryRouter initialEntries={["/research/jobs/job-1"]}>
        <JobsBackLink />
      </MemoryRouter>,
    );

    expect(screen.getByRole("link", { name: /back to jobs/i })).toHaveAttribute(
      "href",
      "/research/jobs?q=https%3A%2F%2Fexample.test%2Fartifact",
    );
  });
});
