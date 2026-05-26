import type { ReactNode } from "react";
import { Banner } from "../ui/dashboard-primitives";
import { canPerform } from "../../lib/research-permissions";
import type { ResearchAction } from "../../lib/research-types";

interface RoleGateProps {
  role: "admin" | "observer" | undefined;
  action: ResearchAction;
  children: ReactNode;
  message?: string;
  title?: string;
}

export function RoleGate({
  role,
  action,
  children,
  message,
  title = "Admin role required",
}: RoleGateProps) {
  if (role && canPerform(role, action)) {
    return <>{children}</>;
  }
  return (
    <Banner tone="info" title={title}>
      {message ?? "This action requires admin access."}
    </Banner>
  );
}
