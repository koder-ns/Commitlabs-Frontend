'use client'

import React, { useEffect, useId, useRef, useState } from 'react'
import { createPortal } from 'react-dom'

const FOCUSABLE_SELECTOR = [
  'button:not([disabled])',
  '[href]',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(', ')

interface DialogProps {
  open: boolean
  title: string
  description?: string
  onClose?: () => void
  initialFocusRef?: React.RefObject<HTMLElement | null>
  children: React.ReactNode
}

export default function Dialog({
  open,
  title,
  description,
  onClose,
  initialFocusRef,
  children,
}: DialogProps) {
  const [mounted, setMounted] = useState(false)
  const containerRef = useRef<HTMLDivElement>(null)
  const titleId = useId()
  const descriptionId = useId()
  const previouslyFocusedRef = useRef<HTMLElement | null>(null)

  useEffect(() => {
    setMounted(true)
    return () => setMounted(false)
  }, [])

  useEffect(() => {
    if (!open) {
      return
    }

    previouslyFocusedRef.current = document.activeElement as HTMLElement | null

    const focusTarget = window.setTimeout(() => {
      initialFocusRef?.current?.focus()
    }, 0)

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose?.()
        return
      }

      if (event.key !== 'Tab' || !containerRef.current) {
        return
      }

      const focusableElements = Array.from(
        containerRef.current.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)
      )

      if (focusableElements.length === 0) {
        return
      }

      const firstElement = focusableElements[0]
      const lastElement = focusableElements[focusableElements.length - 1]

      if (event.shiftKey && document.activeElement === firstElement) {
        event.preventDefault()
        lastElement.focus()
        return
      }

      if (!event.shiftKey && document.activeElement === lastElement) {
        event.preventDefault()
        firstElement.focus()
      }
    }

    document.addEventListener('keydown', handleKeyDown)
    document.body.style.overflow = 'hidden'

    return () => {
      window.clearTimeout(focusTarget)
      document.removeEventListener('keydown', handleKeyDown)
      document.body.style.overflow = ''
      previouslyFocusedRef.current?.focus()
    }
  }, [initialFocusRef, onClose, open])

  if (!open || !mounted) {
    return null
  }

  return createPortal(
    <div
      className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/75 px-4 py-6 backdrop-blur-sm"
      role="presentation"
    >
      <div
        ref={containerRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={description ? descriptionId : undefined}
        className="w-full max-w-xl rounded-[32px] border border-[#0ff0fc26] bg-[#081014] p-6 text-white shadow-[0_32px_120px_rgba(0,0,0,0.55)] sm:p-8"
      >
        <div className="mb-6 space-y-3">
          <h2 id={titleId} className="text-2xl font-semibold tracking-tight text-white sm:text-[2rem]">
            {title}
          </h2>
          {description ? (
            <p id={descriptionId} className="max-w-2xl text-sm leading-6 text-[#d9f9fb]/78 sm:text-[15px]">
              {description}
            </p>
          ) : null}
        </div>

        {children}
      </div>
    </div>,
    document.body
  )
}
