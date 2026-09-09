// Run against deployment with PLAYWRIGHT_MODULE pointing to playwright/index.mjs.
import assert from 'node:assert/strict';
const {webkit}=await import(process.env.PLAYWRIGHT_MODULE||'playwright');
const browser=await webkit.launch();
try {
  for(const [name,width,height] of [['iphone-se',375,667],['iphone',390,844],['iphone-landscape',844,390]]) {
    const context=await browser.newContext({viewport:{width,height},deviceScaleFactor:3,isMobile:true,hasTouch:true});
    const page=await context.newPage();const errors=[],api=[];
    await page.addInitScript(()=>{window.gaiaTest={uploads:0,lines:0};const p=WebGLRenderingContext.prototype;const upload=p.texImage2D,draw=p.drawArrays;p.texImage2D=function(...args){if(args.length===6)window.gaiaTest.uploads++;return upload.apply(this,args)};p.drawArrays=function(...args){if(args[0]===this.LINES&&args[2]>0)window.gaiaTest.lines++;return draw.apply(this,args)}});
    page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(r.url().includes('/gaia/api/'))api.push(r.url())});
    await page.goto(process.env.GAIA_URL||'https://juha.no/gaia/?v=iphone-layout',{waitUntil:'domcontentloaded'});
    const play=page.getByRole('button',{name:'▶ Play',exact:true});await play.waitFor();
    await page.waitForFunction(()=>!document.querySelector('.public-play').disabled,{timeout:30000});
    await page.waitForFunction(()=>{const im=document.querySelector('.public-uit img');return im.complete&&im.naturalWidth>0},undefined,{timeout:20000});
    await page.waitForFunction(()=>window.gaiaTest.uploads>0&&window.gaiaTest.lines>0,undefined,{timeout:30000});
    for(const selector of ['.public-header','.public-playback','.public-uit','.public-header>button']) {
      const box=await page.locator(selector).boundingBox();assert(box&&box.x>=0&&box.y>=0&&box.x+box.width<=width+1&&box.y+box.height<=height+1,`${name}: ${selector} inside viewport`);
    }
    assert(await page.locator('canvas').evaluate(c=>c.clientHeight>100),'globe has usable height');
    const before=await page.locator('time').textContent();await play.tap();
    await page.waitForFunction(t=>document.querySelector('time').textContent!==t,before,{timeout:20000});
    await page.getByRole('button',{name:'❚❚ Pause',exact:true}).tap();
    await page.getByRole('button',{name:'ⓘ Info & credits'}).tap();
    assert(await page.getByRole('dialog').isVisible());
    await page.getByRole('button',{name:'Close information'}).tap();
    await page.getByRole('button',{name:'Latest',exact:true}).tap();
    await page.waitForTimeout(1000);
    assert.equal(await page.locator('canvas').evaluate(c=>c.getContext('webgl').getError()),0,'no WebGL errors');
    await page.screenshot({path:`/tmp/gaia-${name}.png`});
    assert.deepEqual(api,[],'no processing API requests');assert.deepEqual(errors,[],'no JavaScript errors');
    console.log(`${name}: layout, logos, touch playback, information, static-only requests PASS`);
    await context.close();
  }
}finally{await browser.close()}
