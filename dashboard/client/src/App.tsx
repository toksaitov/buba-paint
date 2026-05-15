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
          </Route>
        </Routes>
      </BrowserRouter>
    </QueryClientProvider>
  );
}
