# VoiceTyper 语音输入

基于 [Handy](https://github.com/cjpais/Handy) 开发，感谢原作者及贡献者。
应用代码沿用 MIT 许可证，完整版权声明保留在 [LICENSE](LICENSE)。

## 安装

运行 `VoiceTyper_0.10.1_x64-setup.exe`，按提示安装。仅支持 64 位 Windows。
安装包已经包含 SenseVoiceSmall Q5_K_M 中文识别模型，不需要下载模型或注册账号。
安装程序优先使用电脑已有的 WebView2；若缺少该组件，引导程序需要联网下载安装。
模型直接从安装目录读取，不会复制到用户数据目录。首次启动会预热模型，请稍等片刻；若 Windows 提示麦克风权限，请允许访问。
应用使用独立的数据目录，可以与 Handy 同时安装（同时运行时请避免快捷键冲突）。

## 说话就能输入

1. 打开聊天窗口、文档等，把光标放在要输入的位置。
2. **按住鼠标“后退”侧键说话，松开后等待文字出现。** 绑定期间该侧键不会让浏览器后退。
3. 也可以按住 **Ctrl + 空格** 说话，松开输入。录音中按 **Esc** 取消。

默认识别中文、输出简体字。语音在本机处理，正常听写无需联网，也不会检查更新。
模型默认常驻内存，让后续输入更快。可选模型下载、外部链接及主动配置的 AI 后处理需联网；不使用这些功能即可离线使用。

## 更改设置

点击系统托盘图标打开设置。在“通用”中点击键位，可改为其他键盘键或鼠标侧键。
鼠标侧键旁的“清除”可以释放该按钮，恢复浏览器的后退功能；重置按钮恢复默认值。
也可选择麦克风、切换按住/切换录音方式、语言及简繁体；高级设置可更改模型卸载时间。
电脑没有侧键时，直接使用键盘快捷键即可。

## 卸载

先从托盘菜单退出，再到 Windows“设置 → 应用”找到 **VoiceTyper** 卸载。
卸载时可选择删除应用数据（含识别历史和模型）；保留数据会保留个人设置。

## 模型版权

SenseVoiceSmall 来自 [FunAudioLLM / Alibaba Group](https://huggingface.co/FunAudioLLM/SenseVoiceSmall)，
由 [handy-computer 转换为 GGUF](https://huggingface.co/handy-computer/SenseVoiceSmall-gguf)。
模型并非 MIT 授权，适用 FunASR Model Open Source License Agreement 1.1；
[模型许可证](src-tauri/resources/models/SenseVoiceSmall-LICENSE.txt) 与
[转换说明](src-tauri/resources/models/SenseVoiceSmall-NOTICE.txt) 随安装包一同分发。

## Build notes (English)

See [BUILD.md](BUILD.md) for Windows prerequisites (Rust stable/MSVC, Bun,
CMake, LLVM and Vulkan SDK). Install dependencies with `bun install`.

```powershell
bun run lint
bun run format:check
bun run check:translations
cd src-tauri
cargo clippy
cd ..
bun run tauri build --config C:\path\to\local-unsigned.json
```

An unsigned local config can contain:

```json
{
  "bundle": {
    "createUpdaterArtifacts": false,
    "targets": ["nsis"],
    "windows": { "signCommand": null }
  }
}
```

The Tauri build hook stages SenseVoice Q5 from the catalog's pinned Hugging Face
revision and verifies its byte count and SHA-256 before packaging it as a Tauri
resource. The 172 MB model is not stored in Git. This is a **build-time** download;
the installed app never downloads its default model. For an offline build, put
the verified `SenseVoiceSmall-Q5_K_M.gguf` in `src-tauri/resources/models/` first.
For development, run `bun scripts/prepare-bundled-model.ts` before `bun run tauri dev`.
The Silero VAD resource described in BUILD.md must also be present.
Only WebView2's small bootstrapper is embedded. An existing runtime is reused;
if absent, installing WebView2 requires internet. NSIS uses solid LZMA compression.
The bundled model is loaded directly from installed resources, without an
additional app-data copy.

Output: `src-tauri/target/release/bundle/nsis/VoiceTyper_0.10.1_x64-setup.exe`.
The installed executable is `voicetyper.exe`; application and installer names
are VoiceTyper and the bundle ID is `com.makeller0.voicetyper`.
Updater checks are forced off and no Handy release endpoint is configured.
The Windows webviews disable background networking and remote DNS resolution;
local app/IPC origins are allowed. Optional model downloads and LLM requests use
the Rust backend, not the webview network service.
Existing VoiceTyper settings are retained on upgrades; Chinese defaults apply
to a new data directory only.
