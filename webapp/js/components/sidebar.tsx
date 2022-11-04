import { Immutable } from "immer";
import React from "react";
import { connect } from "react-redux";

import { StoredPlaylist } from "../types/musicbox";
import { WebAppState } from "../types/store";

interface PlaylistButtonProps {
  name: string;
}

function PlaylistButton({ name }: PlaylistButtonProps): React.ReactElement {
  return <div/>;
}

interface SidebarProps {
  playlists: string[];
}

function mapStateToProps(state: Immutable<WebAppState>): SidebarProps {
  return {
    playlists: Object.values(state.appState.storedPlaylists).map((playlist: StoredPlaylist) => playlist.name),
  };
}

class Sidebar extends React.Component<SidebarProps> {
  public render(): React.ReactNode {
    return <div id="sidebar">
      {this.props.playlists.map((playlist: string) => <PlaylistButton key={playlist} name={playlist}/>)}
    </div>;
  }
}

export default connect(mapStateToProps)(Sidebar);
