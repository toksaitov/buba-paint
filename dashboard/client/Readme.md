# buba dashboard

React frontend for monitoring buba trading bots. Displays operator triage, execution readiness, chart-safe equity curves, filtered trades, grouped signals, strategy contribution, and raw logs via a WebSocket-fed dashboard. Installable as a PWA on iPhone, iPad, and Android. Built with React 19, TypeScript, Vite, TanStack React Query, and Zustand.

## Quick Start

```bash
npm install                # install dependencies
npm run dev                # dev server on :3000 (proxies /api + /ws to :3001)
npm test                   # run vitest suite
npm run build              # TypeScript check + Vite production build
npm run lint               # ESLint check
npm run test:e2e           # Playwright E2E (desktop, iPhone, iPad, Android)
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

The dashboard is a Progressive Web App installable from iPhone, iPad, and Android browsers via "Add to Home Screen". It runs in normal standalone app mode with `viewport-fit=cover` and explicit safe-area layout for notches, Dynamic Island devices, older iOS status bars, and Android display cutouts.

On mobile (below 768px), the sidebar shows in compact icon-only mode by default, keeping all navigation one tap away without consuming screen width. Tapping the expand button in the header opens the full labeled drawer as an overlay. Header utilities collapse behind the "More header controls" button so process, mode, theme, notification, and logout controls stay reachable without overflowing the iPhone toolbar. Nav items keep 44px touch targets. The main scroll surface supports a pull-to-refresh gesture that refreshes active dashboard queries without changing trading state. Trade and signal tables switch to a compact card-list layout. The equity chart height adapts across phone and desktop sizes.

Trade notifications use the browser Notification API while the dashboard app or page is running and receiving WebSocket events. When a trade arrives and the page is hidden, a notification appears and clicking it focuses the dashboard where supported. Toggle via the bell icon in the header. Full background push requires Push API, VAPID keys, service-worker push handlers, and server-side subscription storage, and is intentionally not part of the current dashboard.

A service worker (`public/sw.js`) caches the app shell for offline loading. API and WebSocket requests are never cached.

Icon assets are intentionally split by platform. Browser tabs prefer `favicon.svg` for sharp rendering, then fall back to a multi-size PNG/ICO family (`favicon-16x16.png`, `favicon-32x32.png`, `favicon-48x48.png`, `favicon-64x64.png`, and `favicon.ico`) because Safari, Chrome, and Firefox choose competing favicon sources differently. iOS uses an opaque `apple-touch-icon.png` because Web Clips select Apple touch icons instead of standard favicons. Android uses separate normal and maskable manifest icons so adaptive launchers do not add their own white background. Safari pinned tabs use `mask-icon.svg`, which is monochrome by design.

## Pages

- Login: username/password form, authenticates via `POST /api/auth/login`, stores JWT in Zustand/localStorage, redirects to dashboard.
- Overview: operator triage page with the shadow KPI strip, equity curve, current market, open trades, execution/account snapshot, recent outcomes, and alerts when present.
- Execution: mode-aware execution surface. Paper mode shows simulated execution and omits Polymarket `n/a` walls. Live-readonly mode shows venue/account readiness, account truth, reconciliation, venue activity, and gated future controls.
- Logs: raw bot log viewer with ANSI color support, search, severity/source/event-type filters, event-type counts, follow toggle, line count, and wrap toggle. Polls every 5 seconds.
- Equity: full shadow equity curve using chart-safe series data. Timestamp `0` is kept as baseline context and never plotted as a `1970` point.
- Trades: filtered shadow trade table on desktop and compact card list on mobile. Filters cover strategy, side, status, and market.
- Signals: grouped signal bursts by default, with raw signal rows available as a secondary view.
- Strategies: ranked strategy contribution and risk context. The route is `/strategies`; `/stats` redirects for compatibility.

## Architecture

API layer (`lib/api.ts`): typed REST client with `get()` and `post()` helpers. Attaches Bearer token from the Zustand auth store. Automatically clears the token and redirects on 401 responses. Endpoint functions such as `getEquitySeries`, `getSignalGroups`, `getTradingSummary`, `getTrades`, and `getLogs` are thin wrappers over these helpers.

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

Browser E2E: Playwright with desktop Chromium, iPhone SE, iPhone Dynamic Island-sized, iPad Mini, large iPad, and Android Pixel projects. Mocked API/WebSocket harness in `e2e/fixtures.ts`. Desktop tests are skipped on mobile viewports. Mobile-specific tests verify drawer navigation, card layout, standalone app-shell behavior, and safe-area toolbar spacing.

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
    routes.ts                      # grouped nav metadata and compatibility route mapping
    trading-summary.ts             # runtime, trading, health, capability, and alert labels
    utils.ts                       # formatUsd, formatPct, formatTime, cn, pnlColor
    notifications.ts               # browser Notification API wrapper
    chart-colors.ts                # theme-aware color sets for lightweight-charts
  stores/
    auth-store.ts                  # Zustand: token + user, persisted to localStorage
    mobile-nav-store.ts            # Zustand: drawer open/close state
    theme-store.ts                 # Zustand: theme mode (system/light/dark)
  hooks/
    use-auth.ts                    # login, logout, session restore
    use-trades.ts                  # paginated trades query
    use-equity-series.ts           # chart-safe equity series query
    use-signals.ts                 # signals query
    use-signal-groups.ts           # grouped signal bursts query
    use-logs.ts                    # bot logs query (5s refetch)
    use-process-status.ts          # process running status (5s refetch)
    use-live-status.ts             # live-readiness detail query
    use-trading-summary.ts         # execution summary query
    use-live-updates.ts            # WebSocket -> React Query cache invalidation + notifications
    use-media-query.ts             # reactive CSS media query hook
    use-theme.ts                   # dark mode: store + OS detection + .dark class
  pages/
    login.tsx                      # login form
    dashboard.tsx                  # overview: operator triage
    execution.tsx                  # execution state, readiness, and gated controls
    equity.tsx                     # chart-safe full equity curve
    trades.tsx                     # filtered trade table/cards
    signals.tsx                    # grouped signals with raw view
    logs.tsx                       # ANSI-colored log viewer
    stats.tsx                      # visible Strategies page
  components/
    layout/
      app-shell.tsx                # responsive shell: sidebar or drawer, bot selection
      header.tsx                   # bot name, status, controls, notifications, user info
      nav.tsx                      # bot list + page links, touch-friendly
      logo.tsx                     # SVG logo icon
    common/
      protected-route.tsx          # auth guard, redirects to /login
      loading.tsx                  # spinner
    ui/
      dashboard-primitives.tsx     # shared surface, chip, card, alert, and table primitives
    dashboard/
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
  site.webmanifest                 # PWA manifest (standalone, normal + maskable icons)
  favicon.ico                      # multi-size browser tab favicon
  favicon-*.png                    # raster browser tab favicons
  favicon.svg                      # vector source/reference favicon
  apple-touch-icon.png             # iOS home screen icon
  icon-*.png                       # raster icons (48 through 512, plus maskable)
e2e/
  app.spec.ts                      # desktop navigation + auth flows
  mobile-layout.spec.ts            # mobile drawer + card layout tests
  fixtures.ts                      # mock API and WebSocket harness
```
