@echo off
setlocal

echo Temporarily allowing force-push on main...
gh api --method PUT repos/shinyeonjun/visual_map/branches/main/protection/allow_force_pushes -f enabled=true
if errorlevel 1 (
  echo Failed to update branch protection. Enable "Allow force pushes" manually in GitHub Settings ^> Branches ^> main.
  exit /b 1
)

echo Pushing rewritten main...
git push --force origin main
if errorlevel 1 (
  echo Force push failed.
  exit /b 1
)

echo Restoring branch protection...
gh api --method PUT repos/shinyeonjun/visual_map/branches/main/protection/allow_force_pushes -f enabled=false

echo Done. GitHub may take a few minutes to refresh the Contributors list.
