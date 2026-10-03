import {test, expect} from '@playwright/test';
test('real WASM initializes, all styles draft, public export, responsive keyboard board', async ({page}) => {
  const failures: string[] = []; page.on('pageerror', error => failures.push(error.message));
  await page.goto('/'); await expect(page.getByText('Rust 엔진 준비', {exact: true})).toBeVisible();
  await expect(page.getByRole('option', {name: /사람 대 AI/})).toHaveAttribute('disabled', '');
  for (const style of ['normal', 'chaos', 'grand']) {
    await page.getByLabel('게임 스타일').selectOption(style);
    await page.getByRole('button', {name: /새 게임/}).click();
    await expect(page.locator('.draft-grid .card').first()).toBeVisible();
    await page.locator('.draft-grid .card').first().click();
    await expect(page.getByRole('alert')).toHaveCount(0);
    await expect(page.locator('.history-list li').first()).toBeVisible();
  }
  const download = page.waitForEvent('download'); await page.getByRole('button', {name: '조사 자료 ↓', exact: true}).click();
  expect((await download).suggestedFilename()).toBe('augment-chess-public-research.json');
  await page.setViewportSize({width: 390, height: 844});
  await expect(page.getByRole('group', {name: /체스 보드/})).toBeVisible();
  const cell = page.locator('[data-cell="0-0"]'); await cell.focus(); await page.keyboard.press('ArrowRight'); await expect(page.locator('[data-cell="0-1"]')).toBeFocused();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBeTruthy();
  expect(failures).toEqual([]);
});
test('engine download failure is explicit and does not provide fake board', async ({page}) => {
  await page.route('**/wasm/augment_chess_browser_bg.wasm', route => route.fulfill({status: 503, body: 'unavailable'}));
  await page.goto('/'); await expect(page.getByRole('alert')).toContainText('503');
  await expect(page.getByRole('button', {name: /새 게임/})).toBeDisabled();
});
test('manual normal draft completes and a hinted move commits before the next actor plays', async ({page}) => {
  await page.goto('/'); await expect(page.getByText('Rust 엔진 준비', {exact: true})).toBeVisible();
  await page.getByRole('button', {name: /새 게임/}).click();
  await page.locator('.draft-grid .card').nth(1).click();
  await expect(page.getByRole('button', {name: /새 게임/})).toBeEnabled();
  await page.locator('.draft-grid .card').first().click();
  await expect(page.locator('.draft-grid')).toHaveCount(0);
  await expect(page.getByRole('alert')).toHaveCount(0);
  const before = await page.locator('.history-list li').count();
  await page.locator('[data-cell="6-0"]').click();
  await page.locator('.cell.hinted').first().click();
  await expect(page.locator('.history-list li')).toHaveCount(before + 1);
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.locator('.turn')).toHaveText('흑 차례');
});
