// Copy to <work_dir>/<name>/demo.spec.ts. One test.step per scene; one narration clip per scene.
// Import from ./kit only: it carries the engine's Playwright and the demo kit.
// Log in or seed data before demo.intro(): wt mix cuts everything before it.
import { Demo, expect, test } from "./kit"

test("demo", async ({ page }) => {
  const demo = await Demo.create(page)

  // Cards and captions: in the narration's language (walkie:start's Voice line), UI words as the UI spells them.
  await test.step("Intro", async () => {
    await page.goto("/")
    await demo.intro(
      `<div class="tag"><span>TICKET</span><span>PR #NNNN</span><span>FOR …</span></div>
       <h1>What changed</h1>
       <p>One short line</p>`,
      "n0",
    )
  })

  await test.step("Scene 1", async () => {
    const target = page.getByRole("button", { name: "Save" })
    await demo.scrollTo(target)
    await demo.highlight(target) // { spotlight: true } on text-heavy pages, { zoom: true } on busy ones
    await demo.stepCaption("Press <b>Save</b>") // UI words in <b>, spelled as the UI shows them
    await demo.say("n1")
    await demo.pause(1200)
    await demo.click(target)
    await expect(page.getByText("Saved")).toBeVisible() // assert the result, so a broken demo fails
    await demo.untilQuiet()
  })

  await test.step("Summary", async () => {
    await demo.summary(
      `<div class="tag"><span>SUMMARY</span><span>PR #NNNN</span></div>
       <ul>
         <li>What changed</li>
         <li class="warn">What to watch out for</li>
       </ul>`,
      "n9",
    )
    await demo.finish() // writes voice/timeline.json for wt mix
  })
})
