@echo off
chcp 65001 > nul
setlocal

:: ==========================================
:: 設定項目（環境に合わせて変更してください）
:: ==========================================
:: リポジトリのパス（この.cmdファイルと同じ場所にある場合は "." のままでOK）
set REPO_DIR=.

:: プル対象のブランチ名（例: main, master, dev など）
set BRANCH=master

:: ==========================================
:: 処理実行
:: ==========================================
echo [INFO] Git Pull 処理を開始します...
echo ----------------------------------------

:: リポジトリディレクトリへ移動
cd /d "%~dp0%REPO_DIR%"

:: ディレクトリがGitリポジトリか確認
if not exist ".git" (
    echo [ERROR] 指定されたフォルダに .git が見つかりません。
    echo パスを確認してください: %CD%
    echo ----------------------------------------
    pause
    exit /b 1
)

:: リモートの最新情報を取得してローカルブランチを更新
echo [INFO] リモート(%BRANCH%)から最新コードを取得中...
git checkout %BRANCH%
if %ERRORLEVEL% neq 0 (
    echo [ERROR] ブランチ %BRANCH% への切り替えに失敗しました。
    echo ----------------------------------------
    pause
    exit /b %ERRORLEVEL%
)

git pull origin %BRANCH%
if %ERRORLEVEL% neq 0 (
    echo [ERROR] Git Pull 中にエラーが発生しました（コンフリクト等）。
    echo ----------------------------------------
    pause
    exit /b %ERRORLEVEL%
)

echo ----------------------------------------
echo [SUCCESS] 正常に最新の状態へ更新されました！
echo ----------------------------------------

pause
endlocal