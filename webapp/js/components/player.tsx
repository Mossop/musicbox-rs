import React from "react";
import { connect } from "react-redux";

class Player extends React.Component {
  public render(): React.ReactNode {
    return <div id="player"/>;
  }
}

export default connect()(Player);
