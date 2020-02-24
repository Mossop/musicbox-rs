import { StoredPlaylist, Track } from "./musicbox";

export interface PlaybackState {
  track: Track;
  position: number;
  duration: number;
  paused: boolean;
}

export interface AppState {
  playbackState: PlaybackState | null;
  storedPlaylists: Map<string, StoredPlaylist>;
  playlist: Track[];
}
