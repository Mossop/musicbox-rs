import { rootReducer, Deed } from "deeds/immer";
import { Immutable } from "immer";
import { Middleware, applyMiddleware, createStore, Store } from "redux";
import { createLogger } from "redux-logger";

import { fetchState } from "../api";
import { WebAppState } from "../types/store";
import reducer from "./reducer";

async function buildStore(): Promise<Store<Immutable<WebAppState>, Deed>> {
  let appState = await fetchState();

  const middlewares: Middleware[] = [];

  if (process.env.NODE_ENV === "development") {
    middlewares.push(createLogger());
  }

  return createStore(rootReducer(reducer), { appState, },
    applyMiddleware(...middlewares),
  );

}

export default buildStore();
