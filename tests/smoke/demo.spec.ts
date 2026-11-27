// Smoke test for the kit: a static page, no app. Run with `walkie smoke`.
// Exercises intro, step captions, the box, spotlight, focus zoom, a veiled page change and summary.
import { Demo, expect, test } from "./kit"

test("demo kit smoke test", async ({ page }) => {
  const demo = await Demo.create(page)
  await page.goto("file://" + process.env.DEMO_DIR + "/page.html")
  await demo.intro(
    `<div class="tag"><span>SMOKE-0001</span><span>ทดสอบชุดเครื่องมือ</span></div>
     <h1>ตั้งค่าการแจ้งเตือนทางอีเมล สำหรับริสต์แบนด์ ที่ลงทะเบียนแล้ว</h1>
     <p>หน้าทดสอบ ไม่ใช่แอปจริง <code>mono</code></p>`,
    "n0",
  )
  const email = page.locator("#email")
  await demo.highlight(email)
  await demo.stepCaption("กรอกอีเมล แล้วกด <b>“บันทึก”</b>")
  await demo.say("n1")
  await demo.click(email)
  await email.fill("name@example.com")
  await demo.click(page.locator("#save"))
  await expect(page.getByText("บันทึกแล้ว")).toBeVisible()
  await demo.untilQuiet()

  await demo.highlight(page.locator("#terms"), { spotlight: true })
  await demo.stepCaption("หน้าที่มีตัวหนังสือเยอะ ใช้ <b>spotlight</b>")
  await demo.say("n2")
  await demo.untilQuiet()

  await demo.highlight(page.locator("#save"), { zoom: true })
  await demo.stepCaption("หน้าที่ข้อมูลเยอะ ใช้ <b>zoom</b>")
  await demo.say("n3")
  await demo.untilQuiet()

  // A dark page and back through the veil: wt frames must find no flash.
  await demo.goto("file://" + process.env.DEMO_DIR + "/dark.html")
  await demo.caption("เปลี่ยนหน้าผ่าน <b>demo.goto()</b>")
  await expect(page.locator("#dark-title")).toBeVisible()
  await demo.pause(1500)
  await demo.goto("file://" + process.env.DEMO_DIR + "/page.html")
  await demo.pause(800)

  await demo.summary(
    `<div class="tag"><span>สรุป</span><span>SMOKE-0001</span></div>
     <ul>
       <li>กรอบเน้น ชี้ช่องเดียว</li>
       <li>spotlight และ zoom เลือกใช้ได้</li>
       <li class="warn">หน้าทดสอบ ไม่ใช่แอปจริง</li>
     </ul>`,
    "n9",
  )
  await demo.finish()
})
