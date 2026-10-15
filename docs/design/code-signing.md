# 代码签名与公证调研（macOS / Windows / Linux）

> 状态：**调研，不执行**（beta 反馈期，2026-10-15）。实施需要产品负责人决定账号主体与预算（见第 6 节）。
> 来源：Apple Developer Program 页面、Microsoft Learn（Artifact Signing 概述 / 快速入门 / FAQ、SmartScreen reputation）、SignPath Foundation 条款与文档、Certum 商店页面，均于 2026-10-15 查阅；价格与资格以各官网为准。

## 1. 现状与影响

`1.0.0-beta.1` 的全部安装包未签名（SPEC NF-006：代码签名为 P2）。用户看到的影响：

| 平台 | 现象 |
|---|---|
| macOS | Gatekeeper 阻止双击打开（"无法验证开发者"），需右键 → 打开；Apple 芯片上未签名的 arm64 代码根本不能运行，目前靠 `ld` 的 ad-hoc 签名才能启动 |
| Windows | SmartScreen "已保护你的电脑"，需"更多信息 → 仍要运行"；Windows 11 的智能应用控制（Smart App Control）开启时会直接阻止未签名程序；企业策略可禁止继续 |
| Linux | 无系统级拦截；用户只能靠 `SHA256SUMS.txt` 校验完整性，但校验和文件本身没有签名 |

## 2. macOS：Developer ID 签名 + 公证

### 2.1 前提与成本

- **Apple Developer Program**：**99 美元/年**（官网 "$99 annual membership"）。个人或组织均可加入；组织需要 D-U-N-S 编号，证书上显示组织名，个人则显示个人姓名。
- 证书：**Developer ID Application**（签 `.app` 与 `.dmg`）。在 Apple Developer 账号中生成，导出为 `.p12`。
- 公证凭据：**App Store Connect API 密钥**（`.p8` + Key ID + Issuer ID，推荐，可只给 Developer 角色），或 Apple ID + App 专用密码 + Team ID。

### 2.2 流程（每次发布）

1. 对 `Inkra.app/Contents/MacOS/` 下的每个 Mach-O（`inkra`、`inkra-cli`，均为 universal）签名：`codesign --force --options runtime --timestamp --sign "Developer ID Application: <名称> (<TEAMID>)" <file>`。`--options runtime` 即公证要求的 Hardened Runtime；Inkra 没有 JIT、不加载第三方插件，预计不需要额外 entitlements。
2. 对整个 `.app` 签名（同样带 `--options runtime --timestamp`），再用 `codesign --verify --strict --deep` 与 `spctl -a -vv` 自检。
3. 用 `hdiutil` 生成 dmg（现有 `cargo xtask dist` 步骤），对 dmg 签名。
4. 提交公证并等待：`xcrun notarytool submit Inkra-<版本>-universal.dmg --key <AuthKey.p8> --key-id <ID> --issuer <UUID> --wait`（通常几分钟；失败时用 `xcrun notarytool log <id>` 查看原因）。
5. 装订公证票据：`xcrun stapler staple Inkra-<版本>-universal.dmg`（离线也能通过 Gatekeeper），`xcrun stapler validate` 复核。

### 2.3 CI 集成（`release.yml` 的 `macos` 任务）

- GitHub Secrets：`MACOS_DEVELOPER_ID_P12`（base64）、`MACOS_DEVELOPER_ID_P12_PASSWORD`、`MACOS_NOTARY_KEY_P8`（base64）、`MACOS_NOTARY_KEY_ID`、`MACOS_NOTARY_ISSUER`、`MACOS_TEAM_ID`。
- 在 runner 上建临时钥匙串导入证书（`security create-keychain` / `security import` / `security set-key-partition-list`），任务结束删除；不使用第三方 action 也能完成。
- `cargo xtask dist` 增加可选签名钩子：设置 `INKRA_MACOS_SIGN_IDENTITY` 时在打 dmg 前后执行第 1～3 步；公证与装订放在 CI 步骤里（需要网络与凭据，不放进 xtask）。
- `verify-macos` 任务把现在"跳过 `codesign --verify`"改为必须通过 `codesign --verify --deep --strict` 与 `spctl -a -t open --context context:primary-signature -v`。
- 只在 tag 构建时签名与公证；push 演练保持未签名，避免消耗公证额度、暴露凭据给分支构建。

## 3. Windows：Authenticode 签名

### 3.1 规则变化（决定了方案）

- 自 2023 年起，公开信任的代码签名证书（OV 与 EV）的私钥必须保存在 FIPS 140-2 Level 2 及以上的硬件（USB 令牌或云 HSM）中，**不能再导出 `.pfx` 放进 CI Secrets**。GitHub 托管 runner 只能用**云签名服务**，USB 令牌需要自建 runner。
- **EV 证书不再带来 SmartScreen 豁免**。Microsoft Learn："EV certificates no longer bypass SmartScreen … Paying a premium for EV solely to avoid SmartScreen warnings is no longer justified." 签名后，新文件首次下载仍会提示"无法识别的应用"，但会显示已验证的发布者名称，且信誉随下载量在同一发布者身份下累积，不必每个版本从零开始。

### 3.2 可选方案

| 方案 | 成本 | 资格 | 证书主体 | CI 集成 | 评价 |
|---|---|---|---|---|---|
| **Azure Artifact Signing**（原 Trusted Signing） | Basic 起价 **9.99 美元/月**（含 5000 次签名），需付费 Azure 订阅 | 组织限美、加、欧盟、英、澳、新、日、韩、新加坡、瑞士、挪威、以色列；**个人开发者仅限美国、加拿大** | 已验证的组织名或个人姓名 | 官方 `azure/artifact-signing-action@v2`，OIDC 登录，仅 Windows runner；SignTool 支持的文件类型都能签（含 MSI） | 最省事、最便宜；**若主体不在上述国家或地区（例如中国大陆）则不可用** |
| **SignPath Foundation**（开源免费） | **免费**（"For OSS projects, our services are free of charge"） | OSI 许可证（MIT/Apache 符合）、不含专有组件、项目活跃且已有发布、需要可验证的声誉；人工审核 | **SignPath Foundation**（不是项目或个人） | `signpath/github-action-submit-signing-request@v3`：先用 `actions/upload-artifact` 上传待签文件，再提交签名请求；SignPath 校验文件确由本仓库的 GitHub 工作流构建 | 不受地域限制；代价是发布者名称显示为 SignPath Foundation，且需遵守其行为准则（违规可吊销证书） |
| **Certum Open Source Code Signing** | 约 **69 欧元起**（含智能卡与读卡器） | 个人开源开发者 | "Open Source Developer, <姓名>" | 密钥在智能卡上：GitHub 托管 runner 无法使用，需要插着读卡器的**自建 Windows runner** | 便宜、以个人名义，但自建 runner 有运维与安全成本 |
| 商业 OV + 云 HSM（DigiCert KeyLocker、SSL.com eSigner、Sectigo 等） | 数百美元/年级别（以报价为准） | 组织或个人身份验证 | 组织名或个人姓名 | 各家提供 SignTool 插件或 CLI，可在托管 runner 上用 | 地域限制少，成本最高 |

### 3.3 签什么、何时签

- 先签 `inkra.exe`、`inkra-cli.exe`（x64 与 arm64），再由 WiX 打包，最后签 `.msi`；zip 里放已签名的 exe。
- 签名统一用 SHA-256 摘要并加 RFC 3161 时间戳（`signtool sign /fd SHA256 /tr <TSA> /td SHA256`；Artifact Signing 和 SignPath 会自动处理时间戳）。有时间戳的签名在证书到期后仍然有效。
- `cargo xtask dist` 增加 `INKRA_SIGN_CMD` 钩子：在复制 exe 进 zip/MSI 之前、MSI 生成之后各调用一次，传入文件路径。具体用哪个服务由 CI 配置决定，xtask 保持与服务无关。SignPath 需要"上传 artifact → 提交请求 → 下载已签文件"的往返，放在工作流里而不是 xtask 内。
- `verify-windows` 任务增加 `Get-AuthenticodeSignature` 检查 exe 与 msi 的状态为 `Valid`。

## 4. Linux：完整性与来源证明

Linux 发行渠道没有统一的代码签名，建议做两件零成本的事：

1. **GitHub Artifact Attestations**：在 `publish` 任务加 `actions/attest-build-provenance`（需要 `id-token: write`、`attestations: write` 权限），为每个文件生成 Sigstore 签名的构建来源证明。用户可用 `gh attestation verify <文件> --repo CIPFZ/inkra` 验证。三平台的所有文件都适用，免费，不需要长期密钥。
2. **签名校验和文件**（可选）：用项目 GPG 或 minisign 密钥签 `SHA256SUMS.txt`。需要妥善保管一把长期私钥（放 Secrets），并在 README 公布公钥。如果做了第 1 项，这一项价值有限。

另外，Flatpak 若上架 Flathub 由 Flathub 签名；deb/rpm 仓库（APT/YUM 源）需要仓库签名密钥，属于 1.x 分发渠道的话题，不在本调研范围。

## 5. 推荐路线

| 阶段 | 内容 | 成本 | 前提 |
|---|---|---|---|
| A（可随时做） | 所有发布文件加 GitHub Artifact Attestations；README/release notes 增加验证命令 | 0 | 无 |
| B（1.0 正式版前） | macOS Developer ID 签名 + 公证 + 装订；`verify-macos` 改为强制校验 | 99 美元/年 | 产品负责人注册 Apple Developer Program 并提供证书与 API 密钥 |
| C（1.0 正式版前） | Windows 签名：主体在支持地区则用 Azure Artifact Signing，否则申请 SignPath Foundation；都不可行时用 Certum + 自建 runner | 0～约 120 美元/年 | 主体与地区决定方案；SignPath 需要提交申请并等待审核 |

预计改动：`release.yml` 的 `macos`、`windows`、`publish`、`verify-*` 四类任务，`xtask/src/dist.rs` 两个签名钩子，以及 README"已知限制"与 release notes 模板。均不涉及 `unsafe`，也不新增 Rust 依赖。

## 6. 需要产品负责人决定的事项

1. **签名主体**：个人还是组织？这决定证书上显示的名字、Apple 账号类型（组织需要 D-U-N-S）与 Windows 方案的资格。
2. **Windows 方案**：是否接受证书显示 "SignPath Foundation"（免费）；还是付费以自己的名义签（Artifact Signing 需要主体在支持地区；Certum 需要自建 runner）。
3. **预算**：Apple 99 美元/年是 macOS 公证的硬性前提；Windows 0～约 120 美元/年，商业 OV 更高。
4. **时间点**：放在 1.0 正式版之前（推荐，SPEC NF-006 当前写的是 P2，需要上调并回填 SPEC），还是维持 P2 放到 1.x。
5. 阶段 A（Artifact Attestations）不花钱、不需要账号，是否现在就做。

不决策的后果：1.0 正式版继续以未签名形式发布，macOS 与 Windows 用户首次运行都需要手动放行；Windows 11 开启智能应用控制的机器上无法运行。
