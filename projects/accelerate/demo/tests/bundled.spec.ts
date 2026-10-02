import {test, expect} from '@playwright/test';

test('verified HF bundle loads its pinned engine from a nested asset path', async ({page, request}) => {
  const manifest = await (await request.get('source-manifest.json')).json();
  expect(manifest.sourceCommit).toMatch(/^[0-9a-f]{40}$/);
  await page.goto('./');
  await expect(page.getByText('Rust 엔진 준비', {exact: true})).toBeVisible();
  await page.locator('.diagnostics > summary').click();
  await expect(page.locator('.diagnostic-versions')).toContainText(manifest.sourceCommit);
  await expect(page.locator('.diagnostic-content')).not.toContainText('unbundled_preview');
  await expect(page.getByRole('alert')).toHaveCount(0);
  await page.getByRole('button', {name: '새 게임', exact: true}).click();
  await expect(page.locator('.draft-grid .card').first()).toBeVisible();
});

for (const [asset, code] of [
  ['augment_chess_browser_bg.wasm', 'wasm_integrity_failed'],
  ['augment_chess_browser.js', 'binding_integrity_failed'],
]) {
  test(`verified bundle rejects altered ${asset} before engine initialization`, async ({page}) => {
    await page.route(`**/wasm/${asset}`, async route => {
      const response = await route.fetch();
      const changed = Buffer.from(await response.body());
      changed[changed.length - 1] ^= 1;
      await route.fulfill({response, body: changed});
    });
    await page.goto('./');
    await expect(page.getByRole('alert')).toBeVisible();
    await expect(page.getByRole('button', {name: '새 게임', exact: true})).toBeDisabled();
    await page.locator('.diagnostics > summary').click();
    await expect(page.locator('.diagnostic-content')).toContainText(code);
    await expect(page.locator('.draft-grid')).toHaveCount(0);
  });
}
