import { JsonDecoder } from "ts.data.json";

export interface Track {
  path: string;
  title: string;
}

export const TrackDecoder = JsonDecoder.object<Track>({
  path: JsonDecoder.string,
  title: JsonDecoder.string,
}, "Track");

export interface StoredPlaylist {
  name: string;
  tracks: Track[];
}

export const StoredPlaylistDecoder = JsonDecoder.object<StoredPlaylist>({
  name: JsonDecoder.string,
  tracks: JsonDecoder.array(TrackDecoder, "Track[]"),
}, "Track");

export interface PlayState {
  position: number;
  duration: number;
  paused: boolean;
}

export const PlayStateDecoder = JsonDecoder.object<PlayState>({
  position: JsonDecoder.number,
  duration: JsonDecoder.number,
  paused: JsonDecoder.boolean,
}, "Track");

export interface AppState {
  storedPlaylists: Record<string, StoredPlaylist>;
  playlist: Track[];
  playState: PlayState | undefined;
  volume: number;
}

export const AppStateDecoder = JsonDecoder.object<AppState>({
  storedPlaylists: JsonDecoder.dictionary(StoredPlaylistDecoder, "Dict<StoredPlaylist>"),
  playlist: JsonDecoder.array(TrackDecoder, "Track[]"),
  playState: JsonDecoder.optional(PlayStateDecoder),
  volume: JsonDecoder.number,
}, "Track");
