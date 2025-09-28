# FFmpeg 二进制文件说明

## 重要提示

由于 FFmpeg 二进制文件体积较大（超过 GitHub 的 100MB 限制），这些文件不包含在 Git 仓库中。请按照以下步骤手动下载并放置到正确位置。

## 下载地址

### Windows 版本
- 官方下载：https://www.gyan.dev/ffmpeg/builds/
- 推荐版本：ffmpeg-release-essentials.zip
- 文件名：ffmpeg.exe
- 放置路径：`src-tauri/resources/ffmpeg/windows/ffmpeg.exe`

### macOS 版本
- 官方下载：https://evermeet.cx/ffmpeg/
- 或通过 Homebrew：`brew install ffmpeg` 然后复制二进制文件
- 文件名：ffmpeg
- 放置路径：`src-tauri/resources/ffmpeg/macos/ffmpeg`

### Linux 版本
- 通过包管理器安装：
  - Ubuntu/Debian: `sudo apt install ffmpeg`
  - Fedora: `sudo dnf install ffmpeg`
  - Arch: `sudo pacman -S ffmpeg`
- 或下载静态构建版本：https://johnvansickle.com/ffmpeg/
- 文件名：ffmpeg
- 放置路径：`src-tauri/resources/ffmpeg/linux/ffmpeg`

## 目录结构

下载后，确保目录结构如下：
```
src-tauri/resources/ffmpeg/
├── README.md (本文件)
├── windows/
│   └── ffmpeg.exe
├── macos/
│   └── ffmpeg
└── linux/
    └── ffmpeg
```

## 验证安装

下载并放置文件后，可以通过以下命令验证：

```bash
# Windows
./src-tauri/resources/ffmpeg/windows/ffmpeg.exe -version

# macOS
./src-tauri/resources/ffmpeg/macos/ffmpeg -version

# Linux
./src-tauri/resources/ffmpeg/linux/ffmpeg -version
```

## 注意事项

1. **文件权限**：macOS 和 Linux 平台需要确保 ffmpeg 文件有执行权限：
   ```bash
   chmod +x src-tauri/resources/ffmpeg/macos/ffmpeg
   chmod +x src-tauri/resources/ffmpeg/linux/ffmpeg
   ```

2. **版本要求**：建议使用 FFmpeg 4.0 或更高版本以确保功能兼容性。

3. **构建应用**：在构建 Tauri 应用前，请确保对应平台的 FFmpeg 二进制文件已正确放置。

## 开发提示

开发时如果暂时不需要视频处理功能，可以先跳过 FFmpeg 的下载。应用会在缺少 FFmpeg 时给出相应提示。