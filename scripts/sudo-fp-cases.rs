// sudo 密码提示误报探针 —— 构造用例集。
//
// 由 scripts/sudo-fp-probe.sh 与从 handlers/terminal.rs 抽取的真实检测函数拼成，
// 单独编译运行（不需要 cargo、不改动项目源码）。
//
// 术语：
//   output = 远端回显到终端的字节（检测函数的 output_buf）
//   input  = 用户键入的字节（检测函数的 input_buf）
//   命中   = detect_sudo_password_prompt 返回 true → 前端弹提示条 → 按回车注入密码
//
// 预期列约定：
//   HIT  = 期望命中（是真的 sudo 密码提示）
//   MISS = 期望不命中（是误报，或本就是别家的口令提示）

use std::process::ExitCode;

struct Case {
    id: &'static str,
    group: &'static str,
    desc: &'static str,
    output: &'static [u8],
    input: &'static [u8],
    expect_hit: bool,
    /// 若当前行为与预期不符，这行说明为什么
    why: &'static str,
}

const G_TRUE: &str = "真提示";
const G_FP_STRONG: &str = "误报·强信号";
const G_FP_WEAK: &str = "误报·弱信号";
const G_FP_ECHO: &str = "误报·回显瞬态";
const G_GUARD: &str = "护栏·他人口令";
const G_EDGE: &str = "边界";
const G_MISS: &str = "漏报";

const CASES: &[Case] = &[
    // ===== A. 真实 sudo 提示：必须命中 =====
    Case {
        id: "A01",
        group: G_TRUE,
        desc: "Linux 默认提示 [sudo] password for user:",
        output: b"user@host:~$ sudo -l\r\n[sudo] password for user: ",
        input: b"sudo -l\r",
        expect_hit: true,
        why: "基线：最常见的 sudo 密码提示",
    },
    Case {
        id: "A02",
        group: G_TRUE,
        desc: "提示末尾带换行",
        output: b"user@host:~$ sudo ls\r\n[sudo] password for alice:\r\n",
        input: b"sudo ls\r",
        expect_hit: true,
        why: "基线：部分发行版提示后跟换行",
    },
    Case {
        id: "A03",
        group: G_TRUE,
        desc: "本地化中文 [sudo] user 的密码：",
        output: "[sudo] alice 的密码：".as_bytes(),
        input: b"",
        expect_hit: true,
        why: "基线：中文系统（zh_CN.UTF-8）真实提示",
    },
    Case {
        id: "A04",
        group: G_TRUE,
        desc: "西语本地化 [sudo] contraseña de user:",
        output: "[sudo] contraseña de alice:".as_bytes(),
        input: b"",
        expect_hit: true,
        why: "基线：西语系统真实提示",
    },
    Case {
        id: "A05",
        group: G_TRUE,
        desc: "俄语本地化 [sudo] пароль для user:",
        output: "[sudo] пароль для alice:".as_bytes(),
        input: b"",
        expect_hit: true,
        why: "基线：俄语系统真实提示",
    },
    Case {
        id: "A06",
        group: G_TRUE,
        desc: "提示带 ANSI 颜色包裹（提示独占一行，PS1 行在其前）",
        output: b"\x1b[1;32muser@host\x1b[0m:~$ \r\n\x1b[36m[sudo] password for alice:\x1b[0m ",
        input: b"sudo ls\r",
        expect_hit: true,
        why: "基线：带颜色的提示不能漏。sudo 的提示总是独占一行（PS1 行在其前），行首锚定据此判定",
    },
    Case {
        id: "A07",
        group: G_TRUE,
        desc: "sudo -p 'pwd: ' 自定义短提示，紧跟命令回显行",
        output: b"root@web:~# sudo -p 'pwd: ' id\r\npwd: ",
        input: b"sudo -p 'pwd: ' id\r",
        expect_hit: true,
        why: "基线：sudo -p 自定义提示词",
    },
    Case {
        id: "A08",
        group: G_TRUE,
        desc: "doas 风格裸提示 Password:（输入是 doas）",
        output: b"alice@host:~$ doas ls\r\nPassword: ",
        input: b"doas ls\r",
        expect_hit: true,
        why: "基线：BSD doas 的口令提示。裸 Password: 走弱信号路径，须紧跟提权命令回显行",
    },
    Case {
        id: "A09",
        group: G_TRUE,
        desc: "英文提示行尾有多余空格",
        output: b"[sudo] password for alice:   ",
        input: b"sudo ls\r",
        expect_hit: true,
        why: "基线：行尾空格应被容忍",
    },
    Case {
        id: "A10",
        group: G_TRUE,
        desc: "法语本地化 mot de passe",
        output: "[sudo] mot de passe de alice :".as_bytes(),
        input: b"",
        expect_hit: true,
        why: "基线：法语真实提示",
    },
    Case {
        id: "A11",
        group: G_TRUE,
        desc: "德语 Passwort",
        output: "[sudo] Passwort für alice:".as_bytes(),
        input: b"",
        expect_hit: true,
        why: "基线：德语真实提示",
    },
    Case {
        id: "A12",
        group: G_TRUE,
        desc: "葡语 senha",
        output: "[sudo] senha de alice:".as_bytes(),
        input: b"",
        expect_hit: true,
        why: "基线：葡语真实提示",
    },
    Case {
        id: "A13",
        group: G_TRUE,
        desc: "sudoedit 触发提权（token 是 sudoedit）",
        output: b"[sudo] password for alice: ",
        input: b"sudoedit /etc/hosts\r",
        expect_hit: true,
        why: "基线：sudoedit 与 sudo 同属提权命令",
    },
    Case {
        id: "A14",
        group: G_TRUE,
        desc: "输出很长但真提示在最后（历史 >320 字符）",
        output: b"[sudo] password for alice: ",
        input: b"sudo ls\r",
        expect_hit: true,
        why: "基线：检测只看末行，前面历史再长也不影响",
    },

    // ===== B. 强信号误报：末行同时含 sudo/doas + 密码词 =====
    Case {
        id: "B01",
        group: G_FP_STRONG,
        desc: "journalctl 日志行末行含 sudo + password + 冒号",
        output: b"Jan  5 10:00:00 host sudo: pam_unix(sudo:auth): password: ",
        input: b"journalctl\r",
        expect_hit: false,
        why: "误报：日志末行像提示，但命令是 journalctl，与 sudo 无关",
    },
    Case {
        id: "B02",
        group: G_FP_STRONG,
        desc: "man sudo 文档里出现 change your sudo password:",
        output: b"       change your sudo password: \n",
        input: b"man sudo\r",
        expect_hit: false,
        why: "误报：man 手册正文",
    },
    Case {
        id: "B03",
        group: G_FP_STRONG,
        desc: "cat 一个末行是 change sudo password: 的文本文件",
        output: b"root@h:~# cat notes.txt\r\nTODO list\r\nchange sudo password:",
        input: b"cat notes.txt\r",
        expect_hit: false,
        why: "误报：文件内容巧合",
    },
    Case {
        id: "B04",
        group: G_FP_STRONG,
        desc: "cat /etc/sudoers，末行含 SUDO_PASSWORD 配置",
        output: b"root ALL=(ALL) NOPASSWD: ALL\r\nDefaults env_keep += \"SUDO_PASSWORD:\"\r\n",
        input: b"cat /etc/sudoers\r",
        expect_hit: false,
        why: "误报：sudoers 配置里的 SUDO_PASSWORD 变量名",
    },
    Case {
        id: "B05",
        group: G_FP_STRONG,
        desc: "远端程序自己打印 'sudo password:'（伪造提示）",
        output: b"connecting...\nsudo password: ",
        input: b"mytool --connect\r",
        expect_hit: false,
        why: "误报：应用层伪造的假提示，最典型的攻击面",
    },
    Case {
        id: "B06",
        group: G_FP_STRONG,
        desc: "末行 doas + password（与输入无关）",
        output: b"doas password: ",
        input: b"ls\r",
        expect_hit: false,
        why: "误报：末行含 doas 与 password，但与输入无关",
    },
    Case {
        id: "B07",
        group: G_FP_STRONG,
        desc: "visudo 子串被 contains(\"sudo\") 命中",
        output: b"visudo: password: ",
        input: b"ls\r",
        expect_hit: false,
        why: "误报：contains 是子串匹配，visudo/sudoers 都会算提权提及",
    },
    Case {
        id: "B08",
        group: G_FP_STRONG,
        desc: "大写 SUDO PASSWORD 提示（大小写归一后命中）",
        output: b"SUDO PASSWORD FOR ALICE: ",
        input: b"ls\r",
        expect_hit: false,
        why: "误报：任意程序打印的大写提示",
    },
    Case {
        id: "B09",
        group: G_FP_STRONG,
        desc: "末行是 sudo: password: （sudo 前缀式）",
        output: b"sudo: password: ",
        input: b"ls\r",
        expect_hit: false,
        why: "误报：与输入无关的伪提示",
    },
    Case {
        id: "B10",
        group: G_FP_STRONG,
        desc: "JSON 输出末行含 sudo 与 password",
        output: b"{\"user\":\"sudo\",\"password\":\"x\":",
        input: b"curl http://api/x\r",
        expect_hit: false,
        why: "误报：API 返回的 JSON 片段",
    },
    Case {
        id: "B11",
        group: G_FP_STRONG,
        desc: "上一行普通文本，末行 'sudo password:'",
        output: b"hello world\nsudo password:",
        input: b"cat a.txt\r",
        expect_hit: false,
        why: "误报：强信号不看上一行，只看末行",
    },
    Case {
        id: "B12",
        group: G_FP_STRONG,
        desc: "长行末行含 sudo+password+冒号（>24 字符）",
        output: b"a very long line that mentions sudo and password and ends with colon:",
        input: b"cat long.txt\r",
        expect_hit: false,
        why: "误报：强信号路径没有行长上限",
    },

    // ===== C. 弱信号误报：最后一条输入含 sudo/doas token =====
    Case {
        id: "C01",
        group: G_FP_WEAK,
        desc: "grep sudo /etc/x 之后看到 Password:",
        output: b"Password: ",
        input: b"grep sudo /etc/x\r",
        expect_hit: false,
        why: "误报：sudo 只是 grep 的搜索词，不是提权命令",
    },
    Case {
        id: "C02",
        group: G_FP_WEAK,
        desc: "ssh host sudo 之后看到 Password:",
        output: b"Password: ",
        input: b"ssh host sudo\r",
        expect_hit: false,
        why: "误报：sudo 是远端命令参数，不是本地提权",
    },
    Case {
        id: "C03",
        group: G_FP_WEAK,
        desc: "echo sudo 之后看到 Password:",
        output: b"Password: ",
        input: b"echo sudo\r",
        expect_hit: false,
        why: "误报：echo 的参数",
    },
    Case {
        id: "C04",
        group: G_FP_WEAK,
        desc: "sudo docker login → docker 索要 Password:",
        output: b"Username: alice\r\nPassword: ",
        input: b"sudo docker login\r",
        expect_hit: false,
        why: "误报：这是 docker registry 的口令，不是 sudo 口令；填进去等于把主机密码送去 registry",
    },
    Case {
        id: "C05",
        group: G_FP_WEAK,
        desc: "sudo su - → su 索要 Password:",
        output: b"Password: ",
        input: b"sudo su -\r",
        expect_hit: false,
        why: "误报：su 的 root 口令未必等于登录密码",
    },
    Case {
        id: "C06",
        group: G_FP_WEAK,
        desc: "sudo gpg --decrypt → gpg 索要 passphrase(password:)",
        output: b"password: ",
        input: b"sudo gpg --decrypt f.gpg\r",
        expect_hit: false,
        why: "误报：gpg 私钥口令与登录密码无关",
    },
    Case {
        id: "C07",
        group: G_FP_WEAK,
        desc: "sudo ftp host → ftp 索要 Password:",
        output: b"Password: ",
        input: b"sudo ftp h\r",
        expect_hit: false,
        why: "误报：ftp 口令与登录密码无关",
    },
    Case {
        id: "C08",
        group: G_FP_WEAK,
        desc: "sudo redis-cli → redis 索要 Password:",
        output: b"Password: ",
        input: b"sudo redis-cli\r",
        expect_hit: false,
        why: "误报：redis 口令与登录密码无关",
    },
    Case {
        id: "C09",
        group: G_FP_WEAK,
        desc: "sudo vi /etc/hosts 后 vim 内 grep 结果末行 Password:",
        output: b"Password: ",
        input: b"sudo vim /etc/hosts\r",
        expect_hit: false,
        why: "误报：vim 内的搜索结果，不是口令提示",
    },
    Case {
        id: "C10",
        group: G_FP_WEAK,
        desc: "管道里出现 sudo：cat f | sudo tee g",
        output: b"Password: ",
        input: b"cat f | sudo tee g\r",
        expect_hit: false,
        why: "误报：token 在管道右侧，但语义上不是提权请求",
    },

    // ===== D. 回显瞬态：用户正在逐字敲命令 =====
    Case {
        id: "D01",
        group: G_FP_ECHO,
        desc: "正在敲 'sudo chown root:'（输入行以冒号结尾）",
        output: b"root@web:~# sudo chown root:",
        input: b"sudo chown root:",
        expect_hit: false,
        why: "护栏：输入行以冒号结尾 → 是回显不是提示（已覆盖）",
    },
    Case {
        id: "D02",
        group: G_FP_ECHO,
        desc: "正在敲 'sudo grep password x:'（含密码词）",
        output: b"root@web:~# sudo grep password x:",
        input: b"sudo grep password x:",
        expect_hit: false,
        why: "误报：强信号路径不查输入缓冲，回显瞬态未被拦；\n                 用户敲完这条命令回车后，提示条仍挂着，\n                 再按一次回车就把密码打进命令行",
    },
    Case {
        id: "D03",
        group: G_FP_ECHO,
        desc: "正在敲 'sudo cat /etc/sudoers'（无密码词）",
        output: b"root@web:~# sudo cat /etc/sudoers",
        input: b"sudo cat /etc/sudoers",
        expect_hit: false,
        why: "护栏：不含密码词，不命中",
    },
    Case {
        id: "D04",
        group: G_FP_ECHO,
        desc: "正在敲 'echo sudo password:'（无引号，末行以冒号结尾）",
        output: b"root@web:~# echo sudo password:",
        input: b"echo sudo password:",
        expect_hit: false,
        why: "误报：回显 + 强信号双条件叠加",
    },
    Case {
        id: "D05",
        group: G_FP_ECHO,
        desc: "正在敲 'sudo tee /etc/sudoers.d/x'（无密码词）",
        output: b"root@web:~# sudo tee /etc/sudoers.d/x",
        input: b"sudo tee /etc/sudoers.d/x",
        expect_hit: false,
        why: "护栏：不含密码词，不命中",
    },
    Case {
        id: "D06",
        group: G_FP_ECHO,
        desc: "vim 内按 : 命令行（纯冒号行）",
        output: b":",
        input: b"sudo vim /etc/hosts\r:",
        expect_hit: false,
        why: "护栏：纯冒号行不含密码词",
    },
    Case {
        id: "D07",
        group: G_FP_ECHO,
        desc: "正在敲 'sudo mysql -uroot -p'（无冒号无密码词）",
        output: b"root@web:~# sudo mysql -uroot -p",
        input: b"sudo mysql -uroot -p",
        expect_hit: false,
        why: "护栏：不含密码词，不命中",
    },

    // ===== E. 已有护栏：他人口令提示（应保持不命中）=====
    Case {
        id: "E01",
        group: G_GUARD,
        desc: "sudo mysql → Enter password:",
        output: b"Enter password: ",
        input: b"sudo mysql -uroot -p\r",
        expect_hit: false,
        why: "护栏：整行不等于 password: 已有防护",
    },
    Case {
        id: "E02",
        group: G_GUARD,
        desc: "sudo ssh bob@h → bob@h's password:",
        output: b"alice@10.0.0.2's password: ",
        input: b"sudo ssh bob@10.0.0.2\r",
        expect_hit: false,
        why: "护栏：嵌套 ssh 的口令提示已有防护",
    },
    Case {
        id: "E03",
        group: G_GUARD,
        desc: "sudo passwd alice → New password:",
        output: b"New password: ",
        input: b"sudo passwd alice\r",
        expect_hit: false,
        why: "护栏：改密码提示已有防护",
    },
    Case {
        id: "E04",
        group: G_GUARD,
        desc: "sudo psql → Password for user postgres:",
        output: b"Password for user postgres: ",
        input: b"sudo psql -U postgres\r",
        expect_hit: false,
        why: "护栏：psql 口令提示已有防护",
    },
    Case {
        id: "E05",
        group: G_GUARD,
        desc: "sudo cat note.txt 输出末尾是 TODO:",
        output: b"root@web:~# sudo cat note.txt\r\nmeeting at 10\r\nTODO: ",
        input: b"sudo cat note.txt\r",
        expect_hit: false,
        why: "护栏：短冒号行需紧跟 sudo 回显行，已有防护",
    },
    Case {
        id: "E06",
        group: G_GUARD,
        desc: "sudo -l 的 NOPASSWD 行",
        output: b"User alice may run the following commands on host:\r\n    (ALL) NOPASSWD: ALL\r\n",
        input: b"sudo -l\r",
        expect_hit: false,
        why: "护栏：NOPASSWD 行已有防护",
    },
    Case {
        id: "E07",
        group: G_GUARD,
        desc: "sudo -l 权限列表（末行 (ALL : ALL) ALL）",
        output: b"User alice may run the following commands on host:\r\n    (ALL : ALL) ALL\r\n",
        input: b"sudo -l\r",
        expect_hit: false,
        why: "护栏：权限列表已有防护",
    },
    Case {
        id: "E08",
        group: G_GUARD,
        desc: "sudo apt install 的 'The following NEW packages:'",
        output: b"Reading package lists...\r\nThe following NEW packages will be installed:\r\n",
        input: b"sudo apt install foo\r",
        expect_hit: false,
        why: "护栏：apt 列表行已有防护",
    },
    Case {
        id: "E09",
        group: G_GUARD,
        desc: "sudo 提权失败：3 incorrect password attempts",
        output: b"sudo: 3 incorrect password attempts\r\n",
        input: b"sudo ls\r",
        expect_hit: false,
        why: "护栏：失败提示不是索要密码",
    },
    Case {
        id: "E10",
        group: G_GUARD,
        desc: "read -p 'pwd: ' x（自定义提示但不是 sudo）",
        output: b"root@web:~# read -p 'pwd: ' x\r\npwd: ",
        input: b"read -p 'pwd: ' x\r",
        expect_hit: false,
        why: "护栏：非提权命令的自定义提示已有防护",
    },
    Case {
        id: "E11",
        group: G_GUARD,
        desc: "cat sudo.conf（token 是 sudo.conf 不是 sudo）",
        output: b"Password: ",
        input: b"cat sudo.conf\r",
        expect_hit: false,
        why: "护栏：token 精确匹配已有防护",
    },

    // ===== F. 漏报：真实提示但检不出 =====
    Case {
        id: "F01",
        group: G_MISS,
        desc: "sudo -p 'pwd: ' 提示，但输入缓冲已被清空（弱信号失效）",
        output: b"root@web:~# sudo -p 'pwd: ' id\r\npwd: ",
        input: b"",
        expect_hit: false,
        why: "已接受的漏报（宁可不报）：弱信号依赖输入缓冲，命中清空后自定义提示不再命中，\n                 用户手动输入即可。真实会话里该形态几乎不出现：命中后 sudo_buf 一并清空，\n                 同一提示不会二次进入检测",
    },
    Case {
        id: "F02",
        group: G_MISS,
        desc: "裸 Password: 且输入缓冲已被清空（弱信号完全失效）",
        output: b"Password: ",
        input: b"",
        expect_hit: false,
        why: "已接受的漏报（宁可不报）：弱信号依赖输入缓冲。真实重试场景由 [sudo] 锚定的强信号兜住",
    },
    Case {
        id: "F03",
        group: G_MISS,
        desc: "口令输错后重试，提示再次出现（强信号仍能兜住）",
        output: b"[sudo] password for alice: ",
        input: b"",
        expect_hit: true,
        why: "设计要点：[sudo] 行首锚定的强信号不看输入缓冲，重试识别不依赖残留状态",
    },

    // ===== G. 边界 =====
    Case {
        id: "G01",
        group: G_EDGE,
        desc: "空输出",
        output: b"",
        input: b"sudo ls\r",
        expect_hit: false,
        why: "边界：无输出不应命中",
    },
    Case {
        id: "G02",
        group: G_EDGE,
        desc: "输出只有空白",
        output: b"   \r\n  ",
        input: b"sudo ls\r",
        expect_hit: false,
        why: "边界：空白不应命中",
    },
    Case {
        id: "G03",
        group: G_EDGE,
        desc: "提示恰好在 320 字符窗口边界上",
        output: b"[sudo] password for alice: ",
        input: b"sudo ls\r",
        expect_hit: true,
        why: "边界：末行判定不受窗口长度影响",
    },
    Case {
        id: "G04",
        group: G_EDGE,
        desc: "输入缓冲超 512 字节上限（老命令被挤出窗口）",
        output: b"Password: ",
        input: b"echo aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r",
        expect_hit: false,
        why: "边界：超长输入被 push_limited 截断",
    },
    Case {
        id: "G05",
        group: G_EDGE,
        desc: "中文全角冒号 + 密码词（不含 sudo）",
        output: "密码：".as_bytes(),
        input: b"sudo x\r",
        expect_hit: false,
        why: "边界：中文裸提示（弱信号要求整行恰为 password:，中文不满足）",
    },
    Case {
        id: "G06",
        group: G_EDGE,
        desc: "输入行以全角冒号结尾（回显瞬态的另一形态）",
        output: "root@web:~# sudo chown root：".as_bytes(),
        input: "sudo chown root：".as_bytes(),
        expect_hit: false,
        why: "边界：护栏应同时认全角冒号",
    },
    Case {
        id: "G07",
        group: G_EDGE,
        desc: "提示词在行中间而非末尾（password: 后还有内容）",
        output: b"the password: is wrong\r\n",
        input: b"sudo ls\r",
        expect_hit: false,
        why: "边界：末行不以冒号结尾，不应命中",
    },
    Case {
        id: "G08",
        group: G_EDGE,
        desc: "多行输出，末行是短冒号行且上一行含 sudo",
        output: b"sudo something\r\nok: ",
        input: b"sudo something\r",
        expect_hit: true,
        why: "边界：短提示规则要求提示紧跟命令回显行，此处恰好满足 → 正确命中",
    },
    Case {
        id: "G09",
        group: G_EDGE,
        desc: "多行输出，短冒号行与 sudo 回显行之间隔了输出内容",
        output: b"root@web:~# sudo cat note.txt\r\nmeeting at 10\r\nTODO: ",
        input: b"sudo cat note.txt\r",
        expect_hit: false,
        why: "边界：隔着输出内容的短冒号行不应命中（已有防护，见 E05）",
    },
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let strict_mode = args.iter().any(|a| a == "--strict");
    let only: Option<&str> = args.iter().find_map(|a| a.strip_prefix("--only="));

    println!("sudo 密码提示检测 · 构造用例对照表");
    println!("检测函数来自 handlers/terminal.rs 原文抽取，测的是当前源码\n");

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut groups: Vec<&str> = Vec::new();
    for c in CASES {
        if !groups.contains(&c.group) {
            groups.push(c.group);
        }
    }

    for g in groups {
        println!("── {g} ──");
        for c in CASES.iter().filter(|c| c.group == g) {
            if let Some(f) = only {
                if !c.id.contains(f) {
                    continue;
                }
            }
            let hit = detect_sudo_password_prompt(c.output, c.input);
            let ok = hit == c.expect_hit;
            if ok {
                passed += 1;
            } else {
                failed += 1;
            }
            let expect = if c.expect_hit { "HIT " } else { "MISS" };
            let actual = if hit { "HIT " } else { "MISS" };
            let mark = if ok { "  ok  " } else { " FAIL " };
            println!("  [{mark}] {id}  期望 {expect} / 实际 {actual}  {desc}", id = c.id, desc = c.desc);
            if !ok {
                for line in c.why.lines() {
                    println!("           └─ {line}", line = line);
                }
            }
            if strict_mode {
                let s = strict_detect(c.output, c.input);
                let p = proposed_detect(c.output, c.input);
                println!(
                    "           · 现行={} 简单收紧={} 建议方案={}",
                    if hit { "HIT " } else { "MISS" },
                    if s { "HIT " } else { "MISS" },
                    if p { "HIT " } else { "MISS" },
                );
            }
        }
        println!();
    }

    let total = passed + failed;
    println!("合计 {total} 例：符合预期 {passed}，不符合 {failed}");
    if failed > 0 {
        println!();
        println!("不符合预期的用例 = 当前实现与理想行为的差异（误报或漏报）。");
        println!("加 --strict 可看「强信号也纳入输入校验」后的判定，评估修复收益与代价。");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

/// 「收紧后」的参考实现：把强信号也纳入输入缓冲校验 + 回显瞬态校验。
/// 用于评估修复方案的收益与代价（会引入哪些新漏报）。
fn strict_detect(output: &[u8], input: &[u8]) -> bool {
    if !detect_sudo_password_prompt(output, input) {
        return false;
    }
    if !last_input_requests_elevation(input) {
        return false;
    }
    if input_line_echoes_colon(input) {
        return false;
    }
    true
}

/// 「建议方案」的参考实现：在现行逻辑基础上补三处护栏。
///
/// 1. 强信号要求输出末行带 `[sudo]`/`[doas]` 前缀标记（sudo 的权威标记），
///    或末行本身就是"提权 + 口令"的组合形态 —— 而不是任意文本里含这两个词；
/// 2. 弱信号的"最后一条输入含提权 token"必须是**命令首 token**
///    （排除 grep/echo/ssh host sudo 这类把 sudo 当参数的用法）；
/// 3. 回显瞬态：输入末行以冒号结尾时一律不命中（对强信号同样生效）。
fn proposed_detect(output: &[u8], input: &[u8]) -> bool {
    let text = sanitize_prompt_text(output);
    let tail = tail_chars(&text, 320);
    let normalized = tail
        .trim_end_matches(|c| c == '\n' || c == '\r' || c == ' ')
        .to_lowercase();
    if normalized.is_empty() {
        return false;
    }
    let last_line = normalized
        .rsplit('\n')
        .next()
        .unwrap_or(normalized.as_str())
        .trim_end();
    if last_line.is_empty() {
        return false;
    }
    let ends_prompt = last_line.ends_with(':') || last_line.ends_with('：');
    if !ends_prompt {
        return false;
    }

    let has_sudo_marker = last_line.starts_with("[sudo]")
        || last_line.starts_with("[doas]")
        || last_line.contains("[sudo] password")
        || last_line.contains("[doas] password");
    let mentions_elevation = last_line.contains("sudo") || last_line.contains("doas");
    let mentions_password = [
        "password", "密码", "contraseña", "mot de passe", "passwort", "senha", "пароль",
    ]
    .iter()
    .any(|w| last_line.contains(w));

    // 权威标记：sudo 自己的提示块，不看输入缓冲
    if has_sudo_marker && mentions_password {
        return true;
    }

    // 回显瞬态：输入末行以冒号结尾 → 是回显不是提示
    if input_line_echoes_colon(input) {
        return false;
    }

    // 弱信号：要求提权命令是**首 token**
    if !last_input_starts_with_elevation(input) {
        return false;
    }
    if mentions_password {
        return last_line == "password:";
    }
    let prev_line = normalized
        .rsplit('\n')
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .nth(1)
        .unwrap_or("");
    (prev_line.contains("sudo") || prev_line.contains("doas"))
        && last_line.chars().count() <= 24
        && last_line.chars().any(char::is_alphanumeric)
}

/// 提权命令必须出现在命令行的**首 token** 位置（允许前面带 sudo 的 -u/-g 等参数）。
fn last_input_starts_with_elevation(input_buf: &[u8]) -> bool {
    let Some(line) = last_input_line(input_buf) else {
        return false;
    };
    let mut tokens = line.split_whitespace();
    let Some(first) = tokens.next() else {
        return false;
    };
    if matches!(first, "sudo" | "sudoedit" | "doas") {
        return true;
    }
    // sudo -u user cmd / sudo -i 等：首 token 是 sudo 即算
    first == "sudo"
}
