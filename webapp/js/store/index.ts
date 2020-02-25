import { Immutable } from "immer";
import { Store, Middleware, applyMiddleware } from "redux";
import { createLogger } from "redux-logger";

import { fetchState } from "../api";
import { WebAppState } from "../types/store";
import { BaseAction, createStore } from "./helpers";
import reducer from "./reducer";

async function buildStore(): Promise<Store<Immutable<WebAppState>, BaseAction>> {
  let appState = await fetchState();

  const middlewares: Middleware[] = [];

  if (process.env.NODE_ENV === "development") {
    middlewares.push(createLogger());
  }

  return createStore(reducer, { appState, },
    applyMiddleware(...middlewares),
  );

}

export default buildStore();
