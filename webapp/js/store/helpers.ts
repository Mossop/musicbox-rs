import { produce, Immutable } from "immer";
import { PreloadedState, StoreEnhancer, Store, createStore as createReduxStore } from "redux";

type Mapped<I> = {
  [key in keyof I]: unknown;
};
type Mapper<I, O extends Mapped<I>> = <K extends keyof I>(value: I[K], property: K) => O[K];
function objectMap<I, O extends Mapped<I>>(input: I, mapper: Mapper<I, O>): O {
  let output = {} as O;
  for (let [key, value] of Object.entries(input)) {
    output[key] = mapper(value, key as keyof I);
  }
  return output;
}

/**
 * Generic types:
 *   S: Store state.
 *   U: Updated store state.
 *   M: Reducer map.
 *   R: Reducer.
 *   T: Type.
 */

// Simple reducer for a payload.
type Reducer<P, S = unknown, U = S> = (state: S, payload: P) => U;

type ReducerMap<S, U = S> = {
  [type: string]: (state: S, payload: unknown) => U;
};

// Derive the payload type from a reducer function.
type ReducerPayload<R> = R extends Reducer<infer P> ? P : never;

// A map from string type names to payload types.
type PayloadFor<M, T extends keyof M> = ReducerPayload<M[T]>;

// Basic action with known types.
export interface ReducerAction<M> {
  type: keyof M;
  payload: unknown;
}

export type TypedStore<S, M extends ReducerMap<S, unknown>> = Store<S, ReducerAction<M>>;

// Fully typed action.
interface TypedAction<M, T extends keyof M> extends ReducerAction<M> {
  type: T;
  payload: PayloadFor<M, T>;
}

type ActionCreators<M> = {
  [type in keyof M]: (payload: PayloadFor<M, type>) => TypedAction<M, type>;
};

export function reducer<S, M extends ReducerMap<S, S>>(reducerMap: M): Reducer<ReducerAction<M>, S, S> {
  return (state: S, action: ReducerAction<M>): S => {
    if (action.type in reducerMap) {
      return reducerMap[action.type](state, action.payload);
    }
    return state;
  };
}

export function immutableReducer<S, M extends ReducerMap<S, void>>(reducerMap: M): Reducer<ReducerAction<M>, Immutable<S>, Immutable<S>> {
  return (state: Immutable<S>, action: ReducerAction<M>): Immutable<S> => {
    if (action.type in reducerMap) {
      return produce(state, (draft: S) => reducerMap[action.type](draft, action.payload));
    }
    return state;
  };
}

export function actions<S, M extends ReducerMap<S, unknown>>(reducerMap: M): ActionCreators<M> {
  const creator = <T extends keyof M>(_reducer: unknown, type: T): ((payload: PayloadFor<M, T>) => TypedAction<M, T>) => {
    return (payload: PayloadFor<M, T>): TypedAction<M, T> => ({ type, payload, });
  };

  return objectMap(reducerMap, creator);
}
