import { StoredPlaylist } from "../types/musicbox";
import { AppState } from "../types/store";
import { ReducerMap } from "./actions";

const reducer: ReducerMap = {
  UpdateStoredPlaylist(state: AppState, payload: StoredPlaylist): void {
    state.storedPlaylists.set(payload.name, payload);
  },
};

export default reducer;
