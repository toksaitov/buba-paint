import { useState } from "react";
import { Navigate } from "react-router-dom";
import { useAuth } from "../hooks/use-auth";

export function LoginPage() {
  const { isLoggedIn, login } = useAuth();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  if (isLoggedIn) return <Navigate to="/" replace />;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    setLoading(true);
    try {
      await login(username, password);
    } catch {
      setError("Invalid credentials");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-bg">
      <div className="w-full max-w-xs">
        <div className="border border-border p-6 bg-bg">
          <h1 className="text-[15px] font-bold mb-1">buba-paint</h1>
          <p className="text-[11px] text-muted mb-5">
            Sign in to the dashboard
          </p>
          <form onSubmit={handleSubmit} className="space-y-3">
            <div>
              <label className="block text-[11px] font-semibold mb-1">
                Username
              </label>
              <input
                type="text"
                name="username"
                autoComplete="username"
                autoCapitalize="none"
                autoCorrect="off"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                className="w-full px-2.5 py-1.5 text-[13px] border border-border rounded bg-surface"
                autoFocus
                required
              />
            </div>
            <div>
              <label className="block text-[11px] font-semibold mb-1">
                Password
              </label>
              <input
                type="password"
                name="password"
                autoComplete="current-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="w-full px-2.5 py-1.5 text-[13px] border border-border rounded bg-surface"
                required
              />
            </div>
            {error && (
              <div className="text-[11px] text-accent-red font-medium">
                {error}
              </div>
            )}
            <button
              type="submit"
              disabled={loading}
              className="w-full py-1.5 text-[13px] font-semibold bg-text text-bg rounded hover:opacity-90 transition-opacity disabled:opacity-50"
            >
              {loading ? "Signing in..." : "Sign in"}
            </button>
          </form>
        </div>
      </div>
    </div>
  );
}
