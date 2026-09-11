import { h } from "vue"
import type { Component } from "vue"
import { Folder, Link } from "@vicons/fa"
import { NIcon } from "naive-ui"
import type { SftpFile } from "@/types"
// Material Icon Theme 图标集（VS Code 最流行的文件图标主题）：构建期由
// unplugin-icons 编译成 Vue 组件，离线可用、按需打包；彩色 SVG 自带配色。
// 图标名均以 node_modules/@iconify-json/material-icon-theme/icons.json 为准核对过
import IconDoc from "~icons/material-icon-theme/document"
import IconSettings from "~icons/material-icon-theme/settings"
import IconExe from "~icons/material-icon-theme/exe"
import IconKey from "~icons/material-icon-theme/key"
import IconCert from "~icons/material-icon-theme/certificate"
import IconConsole from "~icons/material-icon-theme/console"
import IconPowerShell from "~icons/material-icon-theme/powershell"
import IconVs from "~icons/material-icon-theme/visualstudio"
import IconTable from "~icons/material-icon-theme/table"
import IconSubtitles from "~icons/material-icon-theme/subtitles"
import IconFavicon from "~icons/material-icon-theme/favicon"
import IconCursor from "~icons/material-icon-theme/cursor"
import IconPhotoshop from "~icons/material-icon-theme/adobe-photoshop"
import IconIllustrator from "~icons/material-icon-theme/adobe-illustrator"
import IconFigma from "~icons/material-icon-theme/figma"
import IconSketch from "~icons/material-icon-theme/sketch"
import IconDrawio from "~icons/material-icon-theme/drawio"
import IconChangelog from "~icons/material-icon-theme/changelog"
import IconReadme from "~icons/material-icon-theme/readme"
import IconJar from "~icons/material-icon-theme/jar"
import IconJavaClass from "~icons/material-icon-theme/javaclass"
import IconWasm from "~icons/material-icon-theme/webassembly"
import IconJupyter from "~icons/material-icon-theme/jupyter"
import IconDatabase from "~icons/material-icon-theme/database"
import IconLog from "~icons/material-icon-theme/log"
import IconLib from "~icons/material-icon-theme/lib"
import IconMakefile from "~icons/material-icon-theme/makefile"
import IconCmake from "~icons/material-icon-theme/cmake"
import IconNpm from "~icons/material-icon-theme/npm"
import IconWebpack from "~icons/material-icon-theme/webpack"
import IconTsconfig from "~icons/material-icon-theme/tsconfig"
import IconEditorconfig from "~icons/material-icon-theme/editorconfig"
import IconRobots from "~icons/material-icon-theme/robots"
import IconGitlab from "~icons/material-icon-theme/gitlab"
import IconJenkins from "~icons/material-icon-theme/jenkins"
import IconMercurial from "~icons/material-icon-theme/mercurial"
import IconTerraform from "~icons/material-icon-theme/terraform"
import IconGit from "~icons/material-icon-theme/git"
import IconDocker from "~icons/material-icon-theme/docker"
import IconTex from "~icons/material-icon-theme/tex"
import IconVerilog from "~icons/material-icon-theme/verilog"
import IconWord from "~icons/material-icon-theme/word"
import IconPpt from "~icons/material-icon-theme/powerpoint"
import IconImage from "~icons/material-icon-theme/image"
import IconVideo from "~icons/material-icon-theme/video"
import IconAudio from "~icons/material-icon-theme/audio"
import IconFont from "~icons/material-icon-theme/font"
import IconZip from "~icons/material-icon-theme/zip"
import IconYaml from "~icons/material-icon-theme/yaml"
import IconToml from "~icons/material-icon-theme/toml"
import IconXml from "~icons/material-icon-theme/xml"
import IconHtml from "~icons/material-icon-theme/html"
import IconCss from "~icons/material-icon-theme/css"
import IconSass from "~icons/material-icon-theme/sass"
import IconLess from "~icons/material-icon-theme/less"
import IconJson from "~icons/material-icon-theme/json"
import IconMarkdown from "~icons/material-icon-theme/markdown"
import IconPdf from "~icons/material-icon-theme/pdf"
import IconScheme from "~icons/material-icon-theme/scheme"
import IconLisp from "~icons/material-icon-theme/lisp"
import IconRacket from "~icons/material-icon-theme/racket"
import IconFortran from "~icons/material-icon-theme/fortran"
import IconCoffee from "~icons/material-icon-theme/coffee"
import IconElm from "~icons/material-icon-theme/elm"
import IconAssembly from "~icons/material-icon-theme/assembly"
import IconVala from "~icons/material-icon-theme/vala"
import IconSolidity from "~icons/material-icon-theme/solidity"
import IconMatlab from "~icons/material-icon-theme/matlab"
import IconProlog from "~icons/material-icon-theme/prolog"
import IconAhk from "~icons/material-icon-theme/autohotkey"
import IconVim from "~icons/material-icon-theme/vim"
import IconSwift from "~icons/material-icon-theme/swift"
import IconKotlin from "~icons/material-icon-theme/kotlin"
import IconRuby from "~icons/material-icon-theme/ruby"
import IconPhp from "~icons/material-icon-theme/php"
import IconJava from "~icons/material-icon-theme/java"
import IconGo from "~icons/material-icon-theme/go"
import IconPython from "~icons/material-icon-theme/python"
import IconRust from "~icons/material-icon-theme/rust"
import IconJs from "~icons/material-icon-theme/javascript"
import IconTs from "~icons/material-icon-theme/typescript"
import IconReact from "~icons/material-icon-theme/react"
import IconReactTs from "~icons/material-icon-theme/react-ts"
import IconVue from "~icons/material-icon-theme/vue"
import IconSvelte from "~icons/material-icon-theme/svelte"
import IconC from "~icons/material-icon-theme/c"
import IconCpp from "~icons/material-icon-theme/cpp"
import IconHpp from "~icons/material-icon-theme/hpp"
import IconCs from "~icons/material-icon-theme/csharp"
import IconFs from "~icons/material-icon-theme/fsharp"
import IconObjC from "~icons/material-icon-theme/objective-c"
import IconObjCpp from "~icons/material-icon-theme/objective-cpp"
import IconDart from "~icons/material-icon-theme/dart"
import IconLua from "~icons/material-icon-theme/lua"
import IconPerl from "~icons/material-icon-theme/perl"
import IconHaskell from "~icons/material-icon-theme/haskell"
import IconElixir from "~icons/material-icon-theme/elixir"
import IconErlang from "~icons/material-icon-theme/erlang"
import IconClojure from "~icons/material-icon-theme/clojure"
import IconOcaml from "~icons/material-icon-theme/ocaml"
import IconHaxe from "~icons/material-icon-theme/haxe"
import IconPurescript from "~icons/material-icon-theme/purescript"
import IconR from "~icons/material-icon-theme/r"
import IconJulia from "~icons/material-icon-theme/julia"
import IconNim from "~icons/material-icon-theme/nim"
import IconZig from "~icons/material-icon-theme/zig"
import IconCrystal from "~icons/material-icon-theme/crystal"
import IconGroovy from "~icons/material-icon-theme/groovy"
import IconGradle from "~icons/material-icon-theme/gradle"
import IconScala from "~icons/material-icon-theme/scala"
import IconTwig from "~icons/material-icon-theme/twig"
import IconPug from "~icons/material-icon-theme/pug"
import IconHbs from "~icons/material-icon-theme/handlebars"
import IconEjs from "~icons/material-icon-theme/ejs"
import IconGraphql from "~icons/material-icon-theme/graphql"
import IconProto from "~icons/material-icon-theme/proto"
import IconLicense from "~icons/material-icon-theme/license"

/** 文件类型配色：用于文件夹/软链接等单色图标 */
export const FILE_TYPE_COLORS = {
  dir: "#f1c27d",
  symlink: "#7c5cff",
  file: "#9aa0a6",
}

function extOf(name: string): string {
  const i = name.lastIndexOf(".")
  return i >= 0 ? name.slice(i + 1).toLowerCase() : ""
}

/** 扩展名 -> 图标组件。按类别归组（压缩包/代码/媒体/办公文档），
 *  未命中的回落到通用文件图标 */
const EXT_ICON_MAP: Record<string, Component> = {
  // 图片
  png: IconImage, jpg: IconImage, jpeg: IconImage, gif: IconImage,
  bmp: IconImage, webp: IconImage, svg: IconImage,
  avif: IconImage, tiff: IconImage, heic: IconImage, heif: IconImage,
  wmf: IconImage, emf: IconImage,
  ico: IconFavicon, cur: IconCursor, ani: IconCursor,
  psd: IconPhotoshop, ai: IconIllustrator, sketch: IconSketch,
  fig: IconFigma, drawio: IconDrawio,
  // 视频
  mp4: IconVideo, mkv: IconVideo, avi: IconVideo, mov: IconVideo,
  wmv: IconVideo, flv: IconVideo, webm: IconVideo, m4v: IconVideo,
  mpg: IconVideo, mpeg: IconVideo, ogv: IconVideo, "3gp": IconVideo,
  rmvb: IconVideo, rm: IconVideo, asf: IconVideo,
  // 音频
  mp3: IconAudio, wav: IconAudio, flac: IconAudio, aac: IconAudio,
  ogg: IconAudio, m4a: IconAudio, wma: IconAudio, opus: IconAudio,
  aif: IconAudio, aiff: IconAudio, mid: IconAudio, midi: IconAudio,
  m3u: IconAudio, m3u8: IconAudio, ape: IconAudio, mpc: IconAudio,
  lrc: IconAudio,
  // 压缩包/磁盘镜像
  zip: IconZip, tar: IconZip, gz: IconZip, tgz: IconZip, bz2: IconZip,
  xz: IconZip, "7z": IconZip, rar: IconZip, iso: IconZip, zst: IconZip,
  cab: IconZip,
  // 字体
  ttf: IconFont, otf: IconFont, woff: IconFont, woff2: IconFont,
  eot: IconFont, ttc: IconFont,
  // 证书/密钥
  pem: IconKey, key: IconKey,
  crt: IconCert, cer: IconCert, der: IconCert, p12: IconCert, pfx: IconCert,
  // 可执行/二进制
  exe: IconExe, dll: IconExe, so: IconExe, dylib: IconExe,
  bin: IconExe, msi: IconExe, deb: IconExe, rpm: IconExe,
  appimage: IconExe, apk: IconExe, dmg: IconExe,
  sys: IconExe, com: IconExe, scr: IconExe, cpl: IconExe,
  ocx: IconExe, drv: IconExe, pif: IconExe, dat: IconExe,
  img: IconExe, wim: IconExe, swm: IconExe,
  vhd: IconExe, vhdx: IconExe, vmdk: IconExe, vdi: IconExe,
  gho: IconExe, ova: IconExe, ovf: IconExe,
  msp: IconExe, mst: IconExe, msix: IconExe, appx: IconExe,
  iss: IconExe, bak: IconDoc,
  lib: IconLib, a: IconLib,
  jar: IconJar, war: IconJar, class: IconJavaClass, wasm: IconWasm,
  // 代码：主流
  js: IconJs, mjs: IconJs, cjs: IconJs, jsx: IconReact,
  ts: IconTs, tsx: IconReactTs, vue: IconVue, svelte: IconSvelte,
  py: IconPython, rs: IconRust, go: IconGo, java: IconJava,
  kt: IconKotlin, swift: IconSwift, php: IconPhp, rb: IconRuby,
  cs: IconCs, fs: IconFs, fsi: IconFs, fsx: IconFs,
  c: IconC, h: IconC,
  cpp: IconCpp, cc: IconCpp, cxx: IconCpp, hpp: IconHpp,
  m: IconObjC, mm: IconObjCpp,
  dart: IconDart, lua: IconLua, pl: IconPerl, pm: IconPerl,
  sh: IconConsole, bash: IconConsole, zsh: IconConsole, fish: IconConsole,
  bat: IconConsole, cmd: IconConsole,
  ps1: IconPowerShell, ahk: IconAhk, vim: IconVim,
  // 代码：函数式/其他语言
  scala: IconScala, sc: IconScala,
  hs: IconHaskell, ex: IconElixir, exs: IconElixir,
  erl: IconErlang, hrl: IconErlang,
  clj: IconClojure, cljs: IconClojure, cljc: IconClojure, edn: IconClojure,
  ml: IconOcaml, mli: IconOcaml, hx: IconHaxe, purs: IconPurescript,
  r: IconR, jl: IconJulia, nim: IconNim, zig: IconZig, cr: IconCrystal,
  groovy: IconGroovy, gradle: IconGradle,
  lisp: IconLisp, el: IconLisp, cl: IconLisp,
  scm: IconScheme, rkt: IconRacket,
  f: IconFortran, for: IconFortran, f90: IconFortran, f95: IconFortran,
  coffee: IconCoffee, elm: IconElm,
  asm: IconAssembly, s: IconAssembly,
  vala: IconVala, sol: IconSolidity,
  vhdl: IconVerilog,
  mat: IconMatlab, pro: IconProlog,
  // 标记/样式/模板
  json: IconJson, jsonc: IconJson, json5: IconJson,
  yaml: IconYaml, yml: IconYaml, toml: IconToml, xml: IconXml,
  html: IconHtml, htm: IconHtml, css: IconCss, scss: IconSass,
  sass: IconSass, less: IconLess,
  pug: IconPug, jade: IconPug, hbs: IconHbs,
  handlebars: IconHbs, twig: IconTwig, ejs: IconEjs,
  graphql: IconGraphql, gql: IconGraphql, proto: IconProto,
  tex: IconTex, sty: IconTex, cls: IconTex,
  // 数据
  sql: IconDatabase, db: IconDatabase, db3: IconDatabase,
  sqlite: IconDatabase, sqlite3: IconDatabase, ipynb: IconJupyter,
  // 文档
  md: IconMarkdown, markdown: IconMarkdown, mdx: IconMarkdown,
  pdf: IconPdf, txt: IconDoc, log: IconLog,
  srt: IconSubtitles, ssa: IconSubtitles, ass: IconSubtitles,
  vtt: IconSubtitles,
  doc: IconWord, docx: IconWord, rtf: IconWord, wps: IconWord,
  odt: IconWord, fodt: IconWord,
  xls: IconTable, xlsx: IconTable, csv: IconTable, tsv: IconTable,
  et: IconTable, ods: IconTable, fods: IconTable,
  ppt: IconPpt, pptx: IconPpt, dps: IconPpt, odp: IconPpt, fodp: IconPpt,
  odg: IconImage, fodg: IconImage, odf: IconDoc, odb: IconDatabase,
  // 配置/杂项
  ini: IconSettings, inf: IconSettings,
  conf: IconSettings, cfg: IconSettings, env: IconSettings,
  service: IconSettings, lock: IconSettings, properties: IconSettings,
  config: IconSettings, reg: IconSettings,
  sln: IconVs, csproj: IconVs, vbproj: IconVs,
  vcxproj: IconVs, fsproj: IconVs,
  tf: IconTerraform, tfvars: IconTerraform, hcl: IconTerraform,
  cmake: IconCmake,
  diff: IconGit, patch: IconGit, hgignore: IconMercurial,
  gemspec: IconRuby,
}

/** 特殊文件名（小写精确匹配）-> 图标：覆盖无扩展名/扩展名不表意的知名文件 */
const FILENAME_ICON_MAP: Record<string, Component> = {
  dockerfile: IconDocker,
  ".dockerignore": IconDocker,
  "docker-compose.yml": IconDocker,
  "docker-compose.yaml": IconDocker,
  makefile: IconMakefile,
  "cmakelists.txt": IconCmake,
  ".gitignore": IconGit,
  ".gitattributes": IconGit,
  ".gitmodules": IconGit,
  ".gitconfig": IconGit,
  ".gitlab-ci.yml": IconGitlab,
  jenkinsfile: IconJenkins,
  "cargo.toml": IconRust,
  "cargo.lock": IconRust,
  "go.mod": IconGo,
  "go.sum": IconGo,
  "package.json": IconNpm,
  "package-lock.json": IconNpm,
  ".npmrc": IconNpm,
  ".yarnrc": IconNpm,
  "yarn.lock": IconNpm,
  "webpack.config.js": IconWebpack,
  "webpack.config.ts": IconWebpack,
  "webpack.config.mjs": IconWebpack,
  "webpack.config.cjs": IconWebpack,
  "tsconfig.json": IconTsconfig,
  "jsconfig.json": IconTsconfig,
  ".editorconfig": IconEditorconfig,
  "robots.txt": IconRobots,
  "sitemap.xml": IconXml,
  ".vimrc": IconVim,
  ".bashrc": IconConsole,
  ".zshrc": IconConsole,
  ".bash_profile": IconConsole,
  ".profile": IconConsole,
  gemfile: IconRuby,
  "gemfile.lock": IconRuby,
  "config": IconSettings,
  "hosts": IconDoc,
  changelog: IconChangelog,
  "changelog.md": IconChangelog,
  readme: IconReadme,
  "readme.md": IconReadme,
  "readme.txt": IconReadme,
  license: IconLicense,
  licence: IconLicense,
}

function fileIconOf(fileName: string): Component {
  const lower = fileName.toLowerCase()
  const byName = FILENAME_ICON_MAP[lower]
  if (byName) return byName
  // license.md / changelog.txt 这类：按去扩展名的主名识别
  const dot = lower.lastIndexOf(".")
  if (dot > 0) {
    const base = lower.slice(0, dot)
    if (base === "license" || base === "licence") return IconLicense
    if (base === "readme") return IconReadme
    if (base === "changelog") return IconChangelog
  }
  return EXT_ICON_MAP[extOf(lower)] ?? IconDoc
}

/** SFTP 文件列表通用图标：目录/软链接保持单色主题，文件按扩展名取
 *  Material Icon Theme 彩色图标，本地与远程列表共用 */
export function sftpFileIcon(file: SftpFile) {
  if (file.file_type === "dir") {
    return h(NIcon, { size: 16, color: FILE_TYPE_COLORS.dir }, { default: () => h(Folder) })
  }
  if (file.file_type === "symlink") {
    return h(NIcon, { size: 16, color: FILE_TYPE_COLORS.symlink }, { default: () => h(Link) })
  }
  return h(fileIconOf(file.file_name), { width: 16, height: 16 })
}
