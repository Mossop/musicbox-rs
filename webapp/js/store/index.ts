import { Immutable } from "immer";
import { Middleware, applyMiddleware, createStore } from "redux";
import { createLogger } from "redux-logger";

import { fetchState } from "../api";
import { WebAppState } from "../types/store";
import { immutableReducer, TypedStore } from "./helpers";
import reducer from "./reducer";

async function buildStore(): Promise<TypedStore<Immutable<WebAppState>, typeof reducer>> {
  let appState = await fetchState();

  const middlewares: Middleware[] = [];

  if (process.env.NODE_ENV === "development") {
    middlewares.push(createLogger());
  }

  return createStore(immutableReducer<Immutable<WebAppState>, typeof reducer>(reducer), { appState, },
    applyMiddleware(...middlewares),
  );

}

export default buildStore();
