import DefaultTheme from 'vitepress/theme'
import './custom.css'

let screenshotSequence = 0
let suspendedScreenshotNames = new Map<HTMLElement, string>()

function addScreenshotControls(): void {
  if (document.body.classList.contains('screenshot-lightbox-open')) return
  document.querySelectorAll<HTMLElement>('figure.screenshot').forEach((figure) => {
    const image = figure.querySelector<HTMLImageElement>('img')
    if (!image) return

    if (!figure.style.viewTransitionName) {
      if (!figure.dataset.screenshotId) {
        figure.dataset.screenshotId = String(++screenshotSequence)
      }
      figure.style.viewTransitionName = `screenshot-figure-${figure.dataset.screenshotId}`
    }
    if (figure.querySelector('.screenshot-expand')) return

    const button = document.createElement('button')
    button.className = 'screenshot-expand'
    button.type = 'button'
    button.setAttribute('aria-label', 'View screenshot larger')
    button.title = 'View screenshot larger'
    button.innerHTML = '<span aria-hidden="true">⛶</span>'
    figure.append(button)
  })
}

function installScreenshotLightbox(): void {
  let lightbox: HTMLDivElement | undefined
  let previousFocus: HTMLElement | null = null
  let activeFigure: HTMLElement | undefined
  let originalParent: Node | undefined
  let originalNextSibling: Node | null = null
  let originalClassName = ''
  let originalStyle = ''
  let lightboxResize: (() => void) | undefined

  const transition = (update: () => void, after?: () => void): void => {
    const viewTransitionDocument = document as Document & {
      startViewTransition?: (update: () => void) => { finished: Promise<void> }
    }
    if (!viewTransitionDocument.startViewTransition) {
      update()
      after?.()
      return
    }
    void viewTransitionDocument.startViewTransition(update).finished.then(after)
  }

  const close = (): void => {
    if (!lightbox) return

    const figure = activeFigure
    const finish = (): void => {
      if (lightboxResize) {
        window.removeEventListener('resize', lightboxResize)
        lightboxResize = undefined
      }
      if (figure && originalParent) {
        figure.querySelector('.screenshot-lightbox-close')?.remove()
        figure.className = originalClassName
        if (originalNextSibling?.parentNode === originalParent) {
          originalParent.insertBefore(figure, originalNextSibling)
        } else {
          originalParent.appendChild(figure)
        }
      }
      lightbox?.remove()
      lightbox = undefined
      activeFigure = undefined
      document.body.classList.remove('screenshot-lightbox-open')
      previousFocus?.focus()
      previousFocus = null
      originalParent = undefined
      originalNextSibling = null
    }

    transition(finish, () => {
      if (figure) figure.style.cssText = originalStyle
      suspendedScreenshotNames.forEach((name, otherFigure) => {
        otherFigure.style.viewTransitionName = name
      })
      suspendedScreenshotNames = new Map()
    })
  }

  const open = (figure: HTMLElement): void => {
    close()
    previousFocus = document.activeElement as HTMLElement | null
    activeFigure = figure
    originalParent = figure.parentNode ?? undefined
    originalNextSibling = figure.nextSibling
    originalClassName = figure.className
    originalStyle = figure.style.cssText
    suspendedScreenshotNames = new Map()
    document.querySelectorAll<HTMLElement>('figure.screenshot').forEach((otherFigure) => {
      if (otherFigure === figure) return
      suspendedScreenshotNames.set(otherFigure, otherFigure.style.viewTransitionName)
      otherFigure.style.viewTransitionName = 'none'
    })
    lightbox = document.createElement('div')
    lightbox.className = 'screenshot-lightbox'
    lightbox.setAttribute('role', 'dialog')
    lightbox.setAttribute('aria-modal', 'true')
    lightbox.setAttribute(
      'aria-label',
      figure.querySelector<HTMLImageElement>('img')?.alt || 'Expanded screenshot',
    )

    const panel = document.createElement('div')
    panel.className = 'screenshot-lightbox-panel'
    figure.classList.add('screenshot-lightbox-figure')

    const closeButton = document.createElement('button')
    closeButton.className = 'screenshot-lightbox-close'
    closeButton.type = 'button'
    closeButton.setAttribute('aria-label', 'Close expanded screenshot')
    closeButton.title = 'Close'
    closeButton.innerHTML = '<span aria-hidden="true">×</span>'
    closeButton.addEventListener('click', close)

    figure.append(closeButton)
    lightbox.append(panel)
    lightbox.addEventListener('click', (event) => {
      if (event.target === lightbox) close()
    })
    transition(() => {
      document.body.append(lightbox!)
      panel.append(figure)
      document.body.classList.add('screenshot-lightbox-open')
      const image = figure.querySelector<HTMLImageElement>('img')
      const syncCaptionWidth = (): void => {
        const imageWidth = image?.getBoundingClientRect().width ?? 0
        if (imageWidth > 0) figure.style.setProperty('--lightbox-image-width', `${imageWidth}px`)
      }
      lightboxResize = syncCaptionWidth
      window.addEventListener('resize', syncCaptionWidth)
      syncCaptionWidth()
      window.requestAnimationFrame(syncCaptionWidth)
      image?.addEventListener('load', syncCaptionWidth, { once: true })
    })
    closeButton.focus()
  }

  document.addEventListener('click', (event) => {
    const target = event.target as HTMLElement | null
    const trigger = target?.closest<HTMLElement>('figure.screenshot img, .screenshot-expand')
    if (!trigger) return
    if (lightbox?.contains(trigger)) {
      event.preventDefault()
      close()
      return
    }
    const figure = trigger.closest<HTMLElement>('figure.screenshot')
    if (!figure) return
    event.preventDefault()
    open(figure)
  })

  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') close()
  })
}

export default {
  ...DefaultTheme,
  enhanceApp(context: Parameters<NonNullable<typeof DefaultTheme.enhanceApp>>[0]) {
    DefaultTheme.enhanceApp?.(context)
    if (typeof window === 'undefined') return
    installScreenshotLightbox()
    const decorate = (): void => window.setTimeout(addScreenshotControls, 0)
    decorate()
    const observer = new MutationObserver(decorate)
    observer.observe(document.body, { childList: true, subtree: true })

    const router = context.router as typeof context.router & {
      go: (href?: string) => Promise<void>
    }
    const navigate = router.go.bind(router)
    router.go = async (href?: string): Promise<void> => {
      const viewTransitionDocument = document as Document & {
        startViewTransition?: (update: () => Promise<void>) => { finished: Promise<void> }
      }
      if (!viewTransitionDocument.startViewTransition) {
        await navigate(href)
        return
      }
      await viewTransitionDocument.startViewTransition(() => navigate(href)).finished
    }
  },
}
