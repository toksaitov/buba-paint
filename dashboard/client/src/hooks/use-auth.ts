import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useAuthStore } from "../stores/auth-store";
import * as api from "../lib/api";

export function useAuth() {
  const { token, user, setAuth, logout } = useAuthStore();
  const navigate = useNavigate();

  useEffect(() => {
    if (token && !user) {
      api.getMe().then((u) => setAuth(token, u)).catch(() => logout());
    }
  }, [token, user, setAuth, logout]);

  const doLogin = async (username: string, password: string) => {
    const { token: t, user: u } = await api.login(username, password);
    setAuth(t, u);
    navigate("/");
  };

  const doLogout = () => {
    logout();
    navigate("/login");
  };

  return { token, user, login: doLogin, logout: doLogout, isLoggedIn: !!token };
}
