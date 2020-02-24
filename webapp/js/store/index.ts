import { produce, Immutable } from "immer";
import { Store, Middleware, createStore, applyMiddleware } from "redux";
import { createLogger } from "redux-logger";

import { AppState } from "../types/store";
import { BaseAction } from "./actions";
import reducers from "./reducer";

function buildStore(): Store<Immutable<AppState>, BaseAction> {
  const middlewares: Middleware[] = [];

  if (process.env.NODE_ENV === "development") {
    middlewares.push(createLogger());
  }

  return createStore(
    (state: Immutable<AppState>, action: BaseAction): Immutable<AppState> => {
      return produce(state, (draft: AppState) => {
        // @ts-ignore
        reducers[action.type](draft, action.payload);
      });
    },
    {
      playlist: [],
      playbackState: null,
      storedPlaylists: new Map(),
    },
    applyMiddleware(...middlewares),
  );

}

export default buildStore();
