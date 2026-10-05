import { createContext, useContext, type Dispatch } from "react";
import type { AppAction, AppState } from "./types";

export const AppStateContext = createContext<AppState | null>(null);
export const AppDispatchContext = createContext<Dispatch<AppAction> | null>(
  null,
);

export function useAppState(): AppState {
  const context = useContext(AppStateContext);
  if (!context) {
    throw new Error("useAppState must be used within an AppStateProvider");
  }
  return context;
}

export function useAppDispatch(): Dispatch<AppAction> {
  const context = useContext(AppDispatchContext);
  if (!context) {
    throw new Error("useAppDispatch must be used within an AppStateProvider");
  }
  return context;
}
