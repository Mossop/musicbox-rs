import { produce, Immutable } from "immer";
import { Store, createStore as createReduxStore, PreloadedState, StoreEnhancer } from "redux";

export interface BaseAction {
  type: string;
  payload: unknown;
}

interface AppAction<P> extends BaseAction {
  type: string;
  payload: P;
}

type ActionMarker<P> = (type: string, payload: P) => AppAction<P>;

export function action<P>(): ActionMarker<P> {
  return (type: string, payload: P): AppAction<P> => ({ type, payload, });
}

type Reducer<S, P> = (store: S, payload: P) => void;
type ActionBuilder<P> = (payload: P) => AppAction<P>;
type BuilderFromMarker<T> = T extends ActionMarker<infer P> ? ActionBuilder<P> : never;
type ReducerFromMarker<S, T> = T extends ActionMarker<infer P> ? Reducer<S, P> : never;

type ActionBuilderMap<M extends object> = {
  [k in keyof M]: BuilderFromMarker<M[k]>;
};

type ReducerMap<S, M> = {
  [k in keyof M]: ReducerFromMarker<S, M[k]>;
};

type TopLevelReducer<S> = Reducer<S, BaseAction>;

export function actions<M extends object>(map: M): ActionBuilderMap<M> {
  let result = {};

  for (let [type] of Object.values(map)) {
    result[type] = <P>(payload: P): AppAction<P> => ({
      type, payload,
    });
  }

  return result as ActionBuilderMap<M>;
}

export function reducer<M extends object, S = {}>(reducers: ReducerMap<S, M>): TopLevelReducer<S> {
  return (state: S, action: BaseAction): void => {
    if (action.type in reducers) {
      reducers[action.type](state, action.payload);
    }
  };
}

export function createStore<S, Ext = {}, StateExt = never>(reducer: TopLevelReducer<S>, initialState: PreloadedState<Immutable<S>>, enhancer?: StoreEnhancer<Ext, StateExt>): Store<Immutable<S>, BaseAction> {
  return createReduxStore(
    (state: Immutable<S>, action: BaseAction): Immutable<S> => {
      return produce(state, (draft: S) => reducer(draft, action));
    },
    initialState,
    enhancer
  );
}
