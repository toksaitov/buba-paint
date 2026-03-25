# buba dashboard

React frontend for monitoring buba trading bots. Displays real-time status, equity curves, trades, signals, and logs via a WebSocket-fed dashboard. Built with React 19, TypeScript, Vite, TanStack React Query, and Zustand.

## Quick Start

```bash
npm install                # install dependencies
npm run dev                # dev server on :3000 (proxies /api + /ws to :3001)
npm test                   # run all 131 tests (27 files)
npm run build              # TypeScript check + Vite production build
npm run lint               # ESLint check
```

In production, the dashboard server serves the built frontend as static files. In development, Vite proxies `/api` to `http://localhost:3001` and `/ws` to `ws://localhost:3001`.

## Stack

- React 19 + TypeScript 5.9 + Vite 8: fast builds, HMR, ESM
- TanStack React Query 5: server state management with automatic polling, caching, and cache invalidation on WebSocket messages
- Zustand 5: lightweight client state for auth (token + user, persisted to localStorage)
- Tailwind CSS v4: utility-first styling with custom color tokens via CSS variables (`--bg`, `--text`, `--border`, `--surface`, `--muted`, `--accent-red`, `--accent-green`)
- lightweight-charts 5: fast equity curve rendering (mini + full-page)
- React Router v7: nested routes with `Outlet` context for bot selection
- lucide-react: icon library
- ansi-to-react: ANSI color parsing for bot log rendering

## Pages

- Login: username/password form, authenticates via `POST /api/auth/login`, stores JWT in Zustand/localStorage, redirects to dashboard.
- Dashboard: overview with 4 stat cards (balance, PnL, win rate, trades), mini equity chart, open trades list, recent activity grid.
- Equity: full-height equity curve using lightweight-charts area series with crosshair. Deduplicates timestamps.
- Trades: paginated trade table (50/page default) with time, strategy, side, size, entry price, settlement price, PnL with color coding.
- Signals: signal history with time, strategy, direction, momentum (parsed from JSON metadata), Binance price, UP/DOWN ask prices.
- Logs: bot log viewer with ANSI color support, auto-scroll toggle, configurable line count (100/500/1000). Polls every 5 seconds.
- Stats: bot info + per-strategy breakdown showing trades, wins, losses, win rate, and total PnL.

## Architecture

API layer (`lib/api.ts`): typed REST client with `get()` and `post()` helpers. Attaches Bearer token from the Zustand auth store. Automatically clears the token and redirects on 401 responses. All endpoint functions (`getBots`, `getTrades`, `getBalance`, etc.) are thin wrappers over these helpers.

WebSocket (`lib/ws.ts`): `connectWs(botId, onMessage, onGiveUp)` opens a connection to `/ws/bots/{id}?token=...`. Reconnects with linear backoff (3s * attempt count, max 3 retries). On max failures, calls `onGiveUp` so the hook can disable further attempts. Returns a cleanup function for `useEffect`.

Auth flow: the `useAuthStore` (Zustand) holds `token` (read from localStorage on init) and `user`. The `useAuth` hook wraps login (`api.login` -> `setAuth`), logout (`store.logout` + navigate to `/login`), and session restore (calls `api.getMe` on mount if token exists, logs out on failure). `ProtectedRoute` checks for a token and redirects to `/login` if missing.

Data flow: custom hooks call `api.ts` functions -> TanStack React Query caches responses with configurable stale times and polling intervals (5s for status, logs, process status). The `useLiveUpdates` hook connects the WebSocket and invalidates query keys on message type: `trade` invalidates `["trades"]` + `["bot-status"]`, `balance` invalidates `["balance"]` + `["bot-status"]`, `signal` invalidates `["signals"]`, `status` invalidates `["bot-status"]` + `["process-status"]`.

Bot selection: `AppShell` fetches the bot list via React Query, stores the active bot ID in `sessionStorage`, auto-selects the first bot if none is selected, and passes `{ botId, bot }` to all pages via React Router's `Outlet` context. Pages access it via `useOutletContext()`.

## Testing

131 tests across 27 files.

Framework: vitest + @testing-library/react + jsdom.

Key patterns:
- `vi.mock("../../lib/api")` + `vi.mocked()` for API module mocking
- `renderWithProviders(ui)` wraps in QueryClientProvider + MemoryRouter with a fresh QueryClient (retry: false, staleTime: 0)
- `useAuthStore.getState()` / `useAuthStore.setState()` for Zustand store testing
- `MockWebSocket` class with `simulateOpen()`, `simulateMessage()`, `simulateClose()`, `simulateError()` for WebSocket tests
- `vi.useFakeTimers()` + `vi.advanceTimersByTime()` for reconnection logic
- `vi.mock("react-router-dom")` with `useNavigate` mock for navigation tests

Setup: `src/test/setup.ts` (localStorage polyfill for jsdom + `@testing-library/jest-dom/vitest` matchers).

Shared utils: `src/test/test-utils.tsx` (`renderWithProviders` wrapper).

## Project Structure

```
src/
  App.tsx                          # routes + QueryClientProvider
  main.tsx                         # ReactDOM entry point
  index.css                        # Tailwind imports + CSS variables
  lib/
    api.ts                         # typed REST client (get, post, all endpoints)
    ws.ts                          # WebSocket connection with backoff retry
    types.ts                       # shared TypeScript interfaces
    utils.ts                       # formatUsd, formatPct, formatTime, cn, pnlColor
  stores/
    auth-store.ts                  # Zustand: token + user, persisted to localStorage
  hooks/
    use-auth.ts                    # login, logout, session restore
    use-bot-status.ts              # bot status query (5s refetch)
    use-trades.ts                  # paginated trades query
    use-balance.ts                 # balance history query
    use-signals.ts                 # signals query
    use-logs.ts                    # bot logs query (5s refetch)
    use-process-status.ts          # process running status (5s refetch)
    use-live-updates.ts            # WebSocket -> React Query cache invalidation
  pages/
    login.tsx                      # login form
    dashboard.tsx                  # overview: stat cards, chart, trades, activity
    equity.tsx                     # full equity curve
    trades.tsx                     # paginated trade table
    signals.tsx                    # signal history
    logs.tsx                       # ANSI-colored log viewer
    stats.tsx                      # per-strategy stats
  components/
    layout/
      app-shell.tsx                # sidebar + header + outlet, bot selection
      header.tsx                   # bot name, status, controls, user info
      nav.tsx                      # bot list + page links, collapsible
      logo.tsx                     # SVG logo icon
    common/
      protected-route.tsx          # auth guard, redirects to /login
      loading.tsx                  # spinner
    dashboard/
      stat-card.tsx                # reusable stat box
      mini-chart.tsx               # 120px equity chart
      open-trades.tsx              # open trades list
      recent-activity.tsx          # last 8 settled trades grid
    equity/
      equity-chart.tsx             # full 480px equity chart
    trades/
      trade-table.tsx              # trade table with pagination
    signals/
      signal-table.tsx             # signal log table
  test/
    setup.ts                       # localStorage polyfill + jest-dom matchers
    test-utils.tsx                 # renderWithProviders wrapper
```
