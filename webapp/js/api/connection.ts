import { Handler, handler, MessageToServer } from "../types/api";
import { AppStateDecoder, AppState } from "../types/musicbox";

type Resolve<T> = (value: T | Promise<T>) => void;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type ResponseHandler = (message: any) => void;
type Rejecter = (error: Error) => void;

function connect(): Promise<WebSocket> {
  return new Promise((resolve: Resolve<WebSocket>) => {
    let socket = new WebSocket("/ws");
    let errorHandler = (): void => {
      resolve(connect());
    };

    socket.addEventListener("open", () => {
      socket.removeEventListener("error", errorHandler);
      resolve(socket);
    });

    socket.addEventListener("error", errorHandler);
  });
}

export class Connection {
  private requestId: number = 0;
  private socket: Promise<WebSocket>;
  private requests: Map<number, [ResponseHandler, Rejecter]> = new Map();

  public getState: Handler<AppState> = handler("state", AppStateDecoder);

  public constructor() {
    this.socket = this.connect();
  }

  private async connect(): Promise<WebSocket> {
    let socket = await connect();
    socket.addEventListener("message", () => this.onMessage());
    socket.addEventListener("error", () => this.onError());
    socket.addEventListener("close", () => this.onClose());
    return socket;
  }

  private onError(): void {
    this.socket = this.connect();
  }

  private onClose(): void {
    this.socket = this.connect();
  }

  private onMessage(): void {

  }

  public request(path: string, data: any): Promise<any> {
    return new Promise((resolve: Resolve<any>, reject: Rejecter) => {
      let requestId = this.requestId++;
      this.requests.set(requestId, [ resolve, reject ]);
      this.send({
        type: "Request",
        id: requestId,
        path,
        data,
      });
    });
  }

  private async send(message: MessageToServer): Promise<void> {
    (await this.socket).send(JSON.stringify(message));
  }
}
