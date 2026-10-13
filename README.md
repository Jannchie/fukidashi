<p align="center">
  <img src="src-tauri/icons/icon.svg" width="88" alt="" />
  <h1 align="center">Fukidashi</h1>
  <p align="center">
    <a href="https://github.com/Jannchie/fukidashi/releases/latest"><img src="https://img.shields.io/github/v/release/Jannchie/fukidashi" alt="release" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/github/license/Jannchie/fukidashi" alt="license" /></a>
  </p>
</p>

Fukidashi 实时提取视觉小说中的日文文本，为汉字标注振假名，并调用本地或云端模型翻译。

文本提取基于 [LunaTranslator](https://github.com/HIllya51/LunaTranslator) 的 LunaHook，Fukidashi 只保留阅读所需的部分：

- 从多个钩子来源中自动判断正文，无需手动挑选
- 过滤打开回看（backlog）或快进时重新绘制的旧文本
- 按游戏保存全部原文与译文，重新连接后可继续向上翻阅
- 针对 Sakura、GalTransl 等翻译模型使用其指定的提示词格式

仅支持 Windows。

## 安装

从 [Releases](https://github.com/Jannchie/fukidashi/releases/latest) 下载 `Fukidashi_*_x64-setup.exe` 并运行。新版本发布后，应用顶栏会显示更新按钮。

游戏以管理员权限运行时，Fukidashi 也需要以管理员权限运行。

## 使用

1. 启动游戏，在 Fukidashi 中点击「连接到游戏」并选择游戏窗口
2. 在游戏中推进对话，正文随之显示
3. 连接过的游戏会被记录，下次启动游戏时自动连接

正文来源识别有误时，可在底部的来源栏中手动指定，该选择按游戏保存。

## 翻译

翻译设置接受任意 OpenAI 兼容接口，内置 Ollama、LM Studio、llama.cpp 的地址预设。

本地部署推荐 [Sakura-GalTransl-14B](https://huggingface.co/SakuraLLM/Sakura-GalTransl-14B-v3.8)，Q6_K 量化版本约占用 12 GB 显存。导入 Ollama 后，在翻译设置中选择该模型即可。

## 开发

```sh
pnpm install
pnpm lunahook:fetch   # 下载 VERSION 文件中记录版本的 LunaHook
pnpm tauri dev
```

- `pnpm lunahook:update [tag]`：升级 LunaHook 至最新版本或指定版本
- `pnpm release <patch|minor|major|x.y.z> [--push]`：更新版本号并打 tag，推送后由 GitHub Actions 构建并发布

## License

[GPL-3.0-or-later](LICENSE)。随附的 LunaHook 同样以 GPLv3 发布，见 [NOTICE](src-tauri/resources/lunahook/NOTICE.md)。
