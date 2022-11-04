import { JsonDecoder } from "ts.data.json";

import { Connection } from "../api/connection";

export type Handler<S> = () => Promise<S>;
export type ParamHandler<Q, S> = (data: Q) => Promise<S>;

export function handler<Q, S>(path: string, decoder: JsonDecoder.Decoder<S>): Handler<S> {
  return async function(this: Connection): Promise<S> {
    return decoder.decodePromise(await this.request(path, undefined));
  };
}

export function paramHandler<Q, S>(path: string, decoder: JsonDecoder.Decoder<S>): ParamHandler<Q, S> {
  return async function(this: Connection, data: Q): Promise<S> {
    return decoder.decodePromise(await this.request(path, data));
  };
}

export type Command = {
  type: "PreviousTrack" |
  "NextTrack" |
  "PlayPause" |
  "VolumeUp" |
  "VolumeDown" |
  "Shutdown" |
  "Reload" |
  "Status";
} | {
  type: "StartPlaylist";
  name: string;
  force: boolean;
};

export type Event = {
  type: "PreviousTrack" |
  "NextTrack" |
  "PlayPause" |
  "VolumeUp" |
  "VolumeDown" |
  "Shutdown" |
  "Reload" |
  "Status";
} | {
  type: "PlaybackPosition";
  duration: number;
};

export type MessageFromServer = {
  type: "Response";
  id: number;
  response: any;
} | {
  type: "Event";
  event: Event;
};

export type MessageToServer = {
  type: "Request";
  id: number;
  path: string;
  data: any;
} | {
  type: "Command";
  command: Command;
};
