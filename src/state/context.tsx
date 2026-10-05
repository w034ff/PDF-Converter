import { useReducer, type ReactNode } from "react";
import { appReducer, createInitialAppState } from "./appReducer";
import { AppDispatchContext, AppStateContext } from "./contextBase";
import type { AppState } from "./types";

export interface AppStateProviderProps {
  children: ReactNode;
  initialState?: AppState;
}

export function AppStateProvider({
  children,
  initialState,
}: AppStateProviderProps) {
  const [state, dispatch] = useReducer(
    appReducer,
    initialState ?? createInitialAppState(),
  );

  return (
    <AppStateContext.Provider value={state}>
      <AppDispatchContext.Provider value={dispatch}>
        {children}
      </AppDispatchContext.Provider>
    </AppStateContext.Provider>
  );
}
