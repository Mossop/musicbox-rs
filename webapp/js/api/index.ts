import { AppStateDecoder } from "../types/musicbox";
import { request } from "./helpers";

export const fetchState = request("/api/state", AppStateDecoder);
