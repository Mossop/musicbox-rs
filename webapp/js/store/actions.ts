import { Action } from "redux";

import { StoredPlaylist } from "../types/musicbox";
import { AppState } from "../types/store";

interface ActionTypeMap {
  "UpdateStoredPlaylist": StoredPlaylist;
}

type AppActionTypes = keyof ActionTypeMap;
type AppActionPayloadType<T extends AppActionTypes> = ActionTypeMap[T];

export function isType<T extends AppActionTypes>(action: Action, type: T): action is AppAction<AppActionPayloadType<T>> {
  return action.type == type;
}

export function action<T extends AppActionTypes>(type: T, payload: AppActionPayloadType<T>): AppAction<AppActionPayloadType<T>> {
  return {
    type,
    payload,
  };
}

export interface BaseAction {
  type: AppActionTypes;
}

export interface AppAction<P> extends BaseAction {
  type: AppActionTypes;
  payload: P;
}

export type ReducerMap = {
  [type in AppActionTypes]: (state: AppState, action: AppActionPayloadType<type>) => void;
};
