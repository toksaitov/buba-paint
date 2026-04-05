# buba dashboard

React frontend for monitoring buba trading bots. Displays real-time status, equity curves, trades, signals, and logs via a WebSocket-fed dashboard. Installable as a PWA on iPhone, iPad, and Android with trade notifications. Built with React 19, TypeScript, Vite, TanStack React Query, and Zustand.

## Quick Start

```bash
npm install                # install dependencies
npm run dev                # dev server on :3000 (proxies /api + /ws to :3001)
npm test                   # run vitest suite
npm run build              # TypeScript check + Vite production build
npm run lint               # ESLint check
npm run test:e2e           # Playwright E2E (chromium, iPhone 14, iPad Mini)
```

In production, the dashboard server serves the built frontend as static files. In development, Vite proxies `/api` to `http://localhost:3001` and `/ws` to `ws://localhost:3001`.

## Stack

- React 19 + TypeScript 5.9 + Vite 8: fast builds, HMR, ESM
- TanStack React Query 5: server state management with automatic polling, caching, and cache invalidation on WebSocket messages
- Zustand 5: lightweight client state for auth and mobile nav drawer
- Tailwind CSS v4: utility-first styling with custom color tokens via CSS variables (`--bg`, `--text`, `--border`, `--surface`, `--muted`, `--accent-red`, `--accent-green`)
- lightweight-charts 5: fast equity curve rendering (mini + full-page)
- React Router v7: nested routes with `Outlet` context for bot selection
- lucide-react: icon library
- ansi-to-react: ANSI color parsing for bot log rendering

## Dark Mode

The dashboard supports three theme modes: system (auto-detect from OS), light, and dark. Toggle via the monitor/sun/moon icon in the header. The preference persists in localStorage.

The dark palette uses `#1f2328` as the background (the same color used for active menu selections in light mode), creating visual continuity. Accent colors (red, green, blue) are brightened for contrast on dark backgrounds. Charts recreate with theme-appropriate colors on toggle. The terminal log viewer stays dark in both modes (uses `#161b22`).

An inline script in `index.html` applies the dark class before React hydrates, preventing a white flash on page load.

## Mobile and PWA

The dashboard is a Progressive Web App installable from iPhone, iPad, and Android browsers via "Add to Home Screen". It runs in standalone fullscreen mode with proper notch and dynamic island handling via `viewport-fit=cover` and `env(safe-area-inset-*)` CSS.

On mobile (below 768px), the sidebar shows in compact icon-only mode by default, keeping all navigation one tap away without consuming screen width. Tapping the expand button in the header opens the full labeled drawer as an overlay. Nav items have 44px touch targets per Apple HIG. Trade and signal tables switch to a compact card-list layout. The equity chart height adapts (320px on phones, 480px on desktop).

Trade notifications use the browser Notification API. When a trade arrives via WebSocket and the page is in the background, a notification appears. Toggle via the bell icon in the header. Requires iOS 16.4+ for standalone PWA notification support.

A service worker (`public/sw.js`) caches the app shell for offline loading. API and WebSocket requests are never cached.

## Pages

- Login: username/password form, authenticates via `POST /api/auth/login`, stores JWT in Zustand/localStorage, redirects to dashboard.
- Dashboard: overview with 4 stat cards (balance, PnL, win rate, trades), mini equity chart, open trades list, recent activity.
- Equity: full-height equity curve using lightweight-charts area series with crosshair.
- Trades: paginated trade table on desktop, compact card list on mobile. Shows time, strategy, side, size, entry price, settlement price, PnL.
- Signals: signal history table on desktop, card list on mobile. Shows time, strategy, direction, momentum, BTC price, UP/DOWN ask.
- Logs: bot log viewer with ANSI color support, auto-scroll toggle, configurable line count. Polls every 5 seconds.
- Stats: bot info and per-strategy breakdown showing trades, wins, losses, win rate, and total PnL.

## Architecture

API layer (`lib/api.ts`): typed REST client with `get()` and `post()` helpers. Attaches Bearer token from the Zustand auth store. Automatically clears the token and redirects on 401 responses. All endpoint functions (`getBots`, `getTrades`, `getBalance`, etc.) are thin wrappers over these helpers.

WebSocket (`lib/ws.ts`): `connectWs(botId, onMessage, onGiveUp)` opens a connection to `/ws/bots/{id}?token=...`. Reconnects with linear backoff (3s per attempt, max 3 retries). On max failures, calls `onGiveUp` so the hook can disable further attempts. Returns a cleanup function for `useEffect`.

Auth flow: the `useAuthStore` (Zustand) holds `token` (read from localStorage on init) and `user`. The `useAuth` hook wraps login, logout, and session restore. `ProtectedRoute` checks for a token and redirects to `/login` if missing.

Data flow: custom hooks call `api.ts` functions. TanStack React Query caches responses with configurable stale times and polling intervals (5s for status, logs, process status). The `useLiveUpdates` hook connects the WebSocket and invalidates query keys on message type. Trade messages also trigger browser notifications when the page is in the background.

Layout: `AppShell` detects viewport width via the `useMediaQuery` hook. On desktop (>=768px), the sidebar is a persistent collapsible panel. On mobile (<768px), the sidebar shows in compact icon-only mode by default (always visible, zero extra clicks for navigation). The expand button opens a full labeled drawer overlay controlled by the `useMobileNavStore` (Zustand). The header reorders elements to prevent overflow: status badge before bot name, text labels hidden at narrow widths. Username is hidden on mobile.

Bot selection: `AppShell` fetches the bot list via React Query, stores the active bot ID in `sessionStorage`, auto-selects the first bot if none is selected, and passes `{ botId, bot }` to all pages via React Router's `Outlet` context.

Notifications (`lib/notifications.ts`): opt-in browser notifications for trade events. Permission is requested on first bell-icon click. Enabled state is stored in localStorage. Notifications only fire when the page is hidden.

## Testing

Framework: vitest + @testing-library/react + jsdom.

Key patterns:
- `vi.mock("../../lib/api")` + `vi.mocked()` for API module mocking
- `renderWithProviders(ui)` wraps in QueryClientProvider + MemoryRouter with a fresh QueryClient (retry: false, staleTime: 0)
- `useAuthStore.getState()` / `useAuthStore.setState()` for Zustand store testing
- `MockWebSocket` class with `simulateOpen()`, `simulateMessage()`, `simulateClose()`, `simulateError()` for WebSocket tests
- `vi.useFakeTimers()` + `vi.advanceTimersByTime()` for reconnection logic
- `vi.mock("react-router-dom")` with `useNavigate` mock for navigation tests
- `window.matchMedia` mock in test setup defaults to desktop viewport

Setup: `src/test/setup.ts` (localStorage polyfill, matchMedia mock, `@testing-library/jest-dom/vitest` matchers).

Shared utils: `src/test/test-utils.tsx` (`renderWithProviders` wrapper).

Browser E2E: Playwright with chromium, iPhone 14, and iPad Mini projects. Mocked API/WebSocket harness in `e2e/fixtures.ts`. Desktop tests are skipped on mobile viewports. Mobile-specific tests verify drawer navigation and card layout.

## Project Structure

```
src/
  App.tsx                          # routes + QueryClientProvider
  main.tsx                         # ReactDOM entry + service worker registration
  index.css                        # Tailwind imports + CSS variables + safe areas
  lib/
    api.ts                         # typed REST client (get, post, all endpoints)
    ws.ts                          # WebSocket connection with backoff retry
    types.ts                       # shared TypeScript interfaces
    utils.ts                       # formatUsd, formatPct, formatTime, cn, pnlColor
    notifications.ts               # browser Notification API wrapper
    chart-colors.ts                # theme-aware color sets for lightweight-charts
  stores/
    auth-store.ts                  # Zustand: token + user, persisted to localStorage
    mobile-nav-store.ts            # Zustand: drawer open/close state
    theme-store.ts                 # Zustand: theme mode (system/light/dark)
  hooks/
    use-auth.ts                    # login, logout, session restore
    use-bot-status.ts              # bot status query (5s refetch)
    use-trades.ts                  # paginated trades query
    use-balance.ts                 # balance history query
    use-signals.ts                 # signals query
    use-logs.ts                    # bot logs query (5s refetch)
    use-process-status.ts          # process running status (5s refetch)
    use-live-updates.ts            # WebSocket -> React Query cache invalidation + notifications
    use-media-query.ts             # reactive CSS media query hook
    use-theme.ts                   # dark mode: store + OS detection + .dark class
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
      app-shell.tsx                # responsive shell: sidebar or drawer, bot selection
      header.tsx                   # bot name, status, controls, notifications, user info
      nav.tsx                      # bot list + page links, touch-friendly
      logo.tsx                     # SVG logo icon
    common/
      protected-route.tsx          # auth guard, redirects to /login
      loading.tsx                  # spinner
    dashboard/
      stat-card.tsx                # reusable stat box
      mini-chart.tsx               # 120px equity chart
      open-trades.tsx              # open trades list
      recent-activity.tsx          # last 8 settled trades
    equity/
      equity-chart.tsx             # responsive equity chart (320-480px)
    trades/
      trade-table.tsx              # desktop table + mobile card list
    signals/
      signal-table.tsx             # desktop table + mobile card list
  test/
    setup.ts                       # localStorage + matchMedia polyfills + jest-dom
    test-utils.tsx                 # renderWithProviders wrapper
public/
  sw.js                            # service worker (network-first with offline shell fallback)
  site.webmanifest                 # PWA manifest (standalone, maskable icon)
  favicon.svg                      # vector favicon
  apple-touch-icon.png             # iOS home screen icon
  icon-*.png                       # raster icons (48 through 512, plus maskable)
e2e/
  app.spec.ts                      # desktop navigation + auth flows
  mobile-layout.spec.ts            # mobile drawer + card layout tests
  fixtures.ts                      # mock API and WebSocket harness
```
