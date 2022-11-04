import { JsonDecoder } from "ts.data.json";

type Request<R> = (init?: RequestInit) => Promise<R>;
type TypedRequest<P, R> = (params: P, init?: RequestInit) => Promise<R>;

export function request<R>(path: string, decoder: JsonDecoder.Decoder<R>, defaults?: RequestInit): Request<R> {
  return async (init?: RequestInit): Promise<R> => {
    let response = await fetch(path, Object.assign({}, defaults, init));
    return decoder.decodePromise(await response.json());
  };
}

export function typedRequest<P, R>(path: string, decoder: JsonDecoder.Decoder<R>, defaults?: RequestInit): TypedRequest<P, R> {
  return async (params: P, init?: RequestInit): Promise<R> => {
    let response = await fetch(path, Object.assign({
      body: JSON.stringify(params),
    }, defaults, init));
    return decoder.decodePromise(await response.json());
  };
}
