import { JsonDecoder } from "ts.data.json";

type Request<T> = (init?: RequestInit) => Promise<T>;

export function request<R>(path: string, decoder: JsonDecoder.Decoder<R>, defaults?: RequestInit): Request<R> {
  return async (): Promise<R> => {
    let response = await fetch(path, Object.assign({}, defaults));
    return decoder.decodePromise(await response.json());
  };
}
