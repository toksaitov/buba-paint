import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { AppShell } from "./components/layout/app-shell";
import { ProtectedRoute } from "./components/common/protected-route";
import { LoginPage } from "./pages/login";
import { DashboardPage } from "./pages/dashboard";
import { TradesPage } from "./pages/trades";
import { EquityPage } from "./pages/equity";
import { SignalsPage } from "./pages/signals";
import { LogsPage } from "./pages/logs";
import { StatsPage } from "./pages/stats";
import { ExecutionPage } from "./pages/execution";
import { ConfigPage } from "./pages/config";
import { MachinePage } from "./pages/machine";
import { ResearchOverviewPage } from "./pages/research-overview";
import { ResearchMachinesPage } from "./pages/research-machines";
import { ResearchMachineDetailPage } from "./pages/research-machine-detail";
import { ResearchArtifactsPage } from "./pages/research-artifacts";
import { ResearchArtifactDetailPage } from "./pages/research-artifact-detail";
import { ResearchTransfersPage } from "./pages/research-transfers";
import { ResearchTransferDetailPage } from "./pages/research-transfer-detail";
import { ResearchJobsPage } from "./pages/research-jobs";
import { ResearchJobNewPage } from "./pages/research-job-new";
import { ResearchJobDetailPage } from "./pages/research-job-detail";
import { ResearchReportsPage } from "./pages/research-reports";
import { ResearchReportDetailPage } from "./pages/research-report-detail";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5000,
      retry: 1,
    },
  },
});

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
          <Route path="/login" element={<LoginPage />} />
          <Route
            element={
              <ProtectedRoute>
                <AppShell />
              </ProtectedRoute>
            }
          >
            <Route path="/" element={<DashboardPage />} />
            <Route path="/equity" element={<EquityPage />} />
            <Route path="/trades" element={<TradesPage />} />
            <Route path="/signals" element={<SignalsPage />} />
            <Route path="/logs" element={<LogsPage />} />
            <Route path="/parameters" element={<ConfigPage />} />
            <Route path="/config" element={<Navigate to="/parameters" replace />} />
            <Route path="/machine" element={<MachinePage />} />
            <Route path="/strategies" element={<StatsPage />} />
            <Route path="/stats" element={<Navigate to="/strategies" replace />} />
            <Route path="/execution" element={<ExecutionPage />} />
            <Route path="/trading" element={<Navigate to="/execution" replace />} />
            <Route path="/live" element={<Navigate to="/execution" replace />} />
            <Route path="/research" element={<ResearchOverviewPage />} />
            <Route path="/research/machines" element={<ResearchMachinesPage />} />
            <Route path="/research/machines/:id" element={<ResearchMachineDetailPage />} />
            <Route path="/research/artifacts" element={<ResearchArtifactsPage />} />
            <Route path="/research/artifacts/:id" element={<ResearchArtifactDetailPage />} />
            <Route path="/research/transfers" element={<ResearchTransfersPage />} />
            <Route path="/research/transfers/:id" element={<ResearchTransferDetailPage />} />
            <Route path="/research/jobs" element={<ResearchJobsPage />} />
            <Route path="/research/jobs/new" element={<ResearchJobNewPage />} />
            <Route path="/research/jobs/:id" element={<ResearchJobDetailPage />} />
            <Route path="/research/reports" element={<ResearchReportsPage />} />
            <Route path="/research/reports/:id" element={<ResearchReportDetailPage />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </QueryClientProvider>
  );
}
