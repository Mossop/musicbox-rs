import { Draft } from "immer";

import { AppState } from "../types/musicbox";
import { WebAppState } from "../types/store";

export default {
  setState(state: Draft<WebAppState>, appState: AppState): void {
    state.appState = appState;
  }
};
