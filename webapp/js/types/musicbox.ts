export interface Track {
  path: string;
  title: string;
}

export interface StoredPlaylist {
  name: string;
  tracks: Track[];
}
