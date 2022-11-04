import React from "react";
import ReactDOM from "react-dom";
import { Provider } from "react-redux";

import Player from "./components/player";
import Sidebar from "./components/sidebar";
import store from "./store";

store.then((store) => {
  ReactDOM.render(
    <Provider store={store}>
      <div id="content">
        <Sidebar/>
      </div>
      <Player/>
    </Provider>,
    document.getElementById("app")
  );
});
