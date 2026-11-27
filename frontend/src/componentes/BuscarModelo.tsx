import { Select } from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'

import { api } from '../api'
import { titulo } from '../formato'
import type { ListaModelos } from '../tipos'

/** Busca un modelo del catálogo por su nombre (en el servidor). */
export default function BuscarModelo({
  label,
  placeholder = 'Escribe su nombre…',
  value,
  onChange,
}: {
  label?: string
  placeholder?: string
  value: string | null
  onChange: (id: string | null, nombre: string) => void
}) {
  const [texto, setTexto] = useState('')
  const [buscado] = useDebouncedValue(texto, 300)
  const encontrados = useQuery({
    queryKey: ['buscar-modelos', buscado],
    queryFn: () => api<ListaModelos>(`/modelos?q=${encodeURIComponent(buscado)}&por_pagina=20`),
    enabled: buscado.trim().length > 1,
  })
  const opciones = (encontrados.data?.modelos ?? []).map((m) => ({
    value: String(m.id),
    label: `${titulo(m)}${m.faccion && m.nombre ? ` (${m.faccion})` : ''}`,
  }))
  return (
    <Select
      label={label}
      placeholder={placeholder}
      searchable
      clearable
      searchValue={texto}
      onSearchChange={setTexto}
      filter={({ options }) => options}
      nothingFoundMessage={buscado.trim().length > 1 && !encontrados.isFetching ? 'Nada coincide' : undefined}
      data={opciones}
      value={value}
      onChange={(v) => onChange(v, opciones.find((o) => o.value === v)?.label ?? '')}
    />
  )
}
