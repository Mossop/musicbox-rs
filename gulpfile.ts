/* eslint-env node */
import path from "path";

import { src, dest, parallel, series, watch as watchFiles } from "gulp";
import gulpSass from "gulp-sass";
import named from "vinyl-named";
import { Configuration } from "webpack";
import gulpWebpack from "webpack-stream";

function target(...args: string[]): string {
  return path.join(__dirname, "target", "webapp", ...args);
}

function source(...args: string[]): string {
  return path.join(__dirname, "webapp", ...args);
}

function buildJsConfig(): Configuration {
  return {
    mode: "production",
    resolve: {
      extensions: [".wasm", ".mjs", ".js", ".json", ".ts", ".tsx"]
    },
    output: {
      publicPath: path.join(__dirname, "target", "webapp", "js"),
      filename: "[name].js",
      chunkFilename: "[name].js",
    },
    devtool: "source-map",
    module: {
      rules: [{
        test: /\.(ts|js)x?$/,
        exclude: /(node_modules|bower_components)/,
        use: "ts-loader",
      }],
    },
  };
}

function watchJsConfig(): Configuration {
  let config = buildJsConfig();
  config.watch = true;
  return config;
}

export function watchJs(): NodeJS.ReadWriteStream {
  return src([source("js", "app.tsx")])
    .pipe(named())
    .pipe(gulpWebpack(watchJsConfig()))
    .pipe(dest(target("js")));
}

export function buildJs(): NodeJS.ReadWriteStream {
  return src([source("js", "app.tsx")])
    .pipe(named())
    .pipe(gulpWebpack(buildJsConfig()))
    .pipe(dest(target("js")));
}

export function buildCss(): NodeJS.ReadWriteStream {
  return src([source("css", "app.scss")])
    .pipe(gulpSass().on("error", (e: string) => gulpSass.logError(e)))
    .pipe(dest(target("css")));
}

export function watchCss(): void {
  watchFiles([source("css", "**", "*.scss")], buildCss);
}

export function buildStatic(): NodeJS.ReadWriteStream {
  return src([source("index.html"), source("static", "**", "*")])
    .pipe(dest(target()));
}

export function watchStatic(): void {
  watchFiles([source("**", "*")], buildStatic);
}

export const build = parallel(buildJs, buildCss, buildStatic);
export const watch = parallel(watchJs, series(buildCss, watchCss), series(buildStatic, watchStatic));

export default build;
