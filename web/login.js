// GitHub OAuth 配置
const GITHUB_CLIENT_ID = 'Ov23li3Enwr4Yal78xRk';
const GITHUB_REPO_OWNER = 'xiaoshihou514';
const GITHUB_REPO_NAME = 'daboyi';
const GITHUB_API_URL = 'https://api.github.com';

// GitHub Personal Access Token 创建页面
const GITHUB_TOKEN_URL = 'https://github.com/settings/tokens/new';

// 存储键名
const STORAGE_KEYS = {
  ACCESS_TOKEN: 'github_access_token',
  USER_INFO: 'github_user_info',
  LOGIN_STATUS: 'github_login_status'
};

// DOM 元素
let elements = {};

// 初始化DOM元素
function initElements() {
  elements = {
    loginContainer: document.getElementById('login-container'),
    loadingContainer: document.getElementById('loading-container'),
    loginButton: document.getElementById('github-login-btn'),
    statusMessage: document.getElementById('status-message'),
    starPrompt: document.getElementById('star-prompt'),
    retryButton: document.getElementById('retry-btn')
  };
}

// 初始化
function init() {
  // 检查本地存储中的登录状态
  const savedToken = localStorage.getItem(STORAGE_KEYS.ACCESS_TOKEN);
  if (savedToken) {
    checkStarStatus(savedToken);
  } else {
    showLogin();
  }
}

// 显示登录界面
function showLogin() {
  if (elements.loginContainer) {
    elements.loginContainer.style.display = 'flex';
  }
  if (elements.loadingContainer) {
    elements.loadingContainer.style.display = 'none';
  }
}

// 显示加载界面
function showLoading界面() {
  if (elements.loginContainer) {
    elements.loginContainer.style.display = 'none';
  }
  if (elements.loadingContainer) {
    elements.loadingContainer.style.display = 'block';
  }
}

// 显示状态消息（支持HTML）
function showStatus(message, type = 'info') {
  if (elements.statusMessage) {
    elements.statusMessage.innerHTML = message;
    elements.statusMessage.className = `status-message ${type}`;
    elements.statusMessage.style.display = 'block';
  }
}

// 隐藏状态消息
function hideStatus() {
  if (elements.statusMessage) {
    elements.statusMessage.style.display = 'none';
  }
}

// 显示星标提示
function showStarPrompt() {
  if (elements.starPrompt) {
    elements.starPrompt.style.display = 'block';
  }
  if (elements.retryButton) {
    elements.retryButton.style.display = 'block';
  }
}

// 隐藏星标提示
function hideStarPrompt() {
  if (elements.starPrompt) {
    elements.starPrompt.style.display = 'none';
  }
  if (elements.retryButton) {
    elements.retryButton.style.display = 'none';
  }
}

// 处理登录按钮点击
function handleLoginClick() {
  const tokenUrl = `${GITHUB_TOKEN_URL}?scopes=public_repo&description=dby`;
  window.open(tokenUrl, '_blank');
  showTokenInput();
}

// 显示token输入框
function showTokenInput() {
  const statusEl = elements.statusMessage;
  if (statusEl) {
    statusEl.innerHTML = `
      <p>请在打开的页面中创建一个令牌（需要勾选 <strong>public_repo</strong> 权限），然后复制粘贴到下方：</p>
      <div style="display: flex; gap: 10px; margin-top: 10px;">
        <input type="text" id="github-token-input" placeholder="ghp_xxxxxxxxxxxx" style="flex: 1; padding: 8px; border: 1px solid rgba(255,255,255,0.2); border-radius: 4px; background: rgba(255,255,255,0.1); color: #f4f7fb;">
        <button id="submit-token-btn" style="padding: 8px 16px; border: 1px solid rgba(255,255,255,0.2); border-radius: 4px; background: rgba(255,255,255,0.1); color: #f4f7fb; cursor: pointer;">提交</button>
      </div>
    `;
    statusEl.className = 'status-message info';
    statusEl.style.display = 'block';

    const inputEl = document.getElementById('github-token-input');
    const btnEl = document.getElementById('submit-token-btn');

    if (btnEl) {
      btnEl.addEventListener('click', () => {
        const token = inputEl.value.trim();
        if (token) {
          submitToken(token);
        }
      });
    }

    if (inputEl) {
      inputEl.addEventListener('keypress', (e) => {
        if (e.key === 'Enter') {
          const token = inputEl.value.trim();
          if (token) {
            submitToken(token);
          }
        }
      });
    }
  }
}

// 提交token
async function submitToken(token) {
  localStorage.setItem(STORAGE_KEYS.ACCESS_TOKEN, token);
  await checkStarStatus(token);
}

// 检查星标状态
async function checkStarStatus(token) {
  showStatus('正在检查星标状态...', 'info');

  try {
    const response = await fetch(
      `${GITHUB_API_URL}/user/starred/${GITHUB_REPO_OWNER}/${GITHUB_REPO_NAME}`,
      {
        headers: {
          'Authorization': `token ${token}`,
          'Accept': 'application/vnd.github.v3+json'
        }
      }
    );

    if (response.status === 204) {
      showStatus('验证通过！正在加载编辑器...', 'success');
      hideStarPrompt();
      setTimeout(async () => {
        showLoading界面();
        await loadWasmApp();
      }, 1000);
    } else if (response.status === 401) {
      localStorage.removeItem(STORAGE_KEYS.ACCESS_TOKEN);
      showStatus('Token无效，请重新登录', 'error');
      showLogin();
    } else {
      showStatus('给我一个星标嘛', 'warning');
      showStarPrompt();
    }
  } catch (error) {
    showStatus('检查星标状态失败，请重试', 'error');
    console.error('Error checking star status:', error);
  }
}

// 处理重试按钮点击
function handleRetryClick() {
  const token = localStorage.getItem(STORAGE_KEYS.ACCESS_TOKEN);
  if (token) {
    checkStarStatus(token);
  } else {
    showLogin();
  }
}

// 初始化事件监听器
function initEventListeners() {
  if (elements.loginButton) {
    elements.loginButton.addEventListener('click', handleLoginClick);
  }
  if (elements.retryButton) {
    elements.retryButton.addEventListener('click', handleRetryClick);
  }
}

// 动态加载WASM应用
async function loadWasmApp() {
  try {
    const script = document.createElement('script');
    script.src = 'client.js';
    script.type = 'module';
    script.defer = true;

    await new Promise((resolve, reject) => {
      script.onload = resolve;
      script.onerror = reject;
      document.head.appendChild(script);
    });

    console.log('WASM application loading started');
  } catch (error) {
    console.error('Error loading WASM application:', error);
    showStatus('加载编辑器失败，请刷新页面重试', 'error');
  }
}

// 导出公共函数
window.GitHubLogin = {
  init,
  login: handleLoginClick,
  checkStarStatus,
  loadWasmApp
};

// 页面加载完成后初始化
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', function() {
    initElements();
    initEventListeners();
    init();
  });
} else {
  initElements();
  initEventListeners();
  init();
}
